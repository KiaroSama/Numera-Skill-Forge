use crate::{
    Error, Result, adapters, build, install,
    model::{Package, Source},
    registry, storage, sync, transaction, validation,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn execute(root: &Path, request: Value) -> Result<Value> {
    if !root.is_absolute() {
        return Err(Error::new("workspace", "Select an absolute workspace path"));
    }
    if !root.is_dir() {
        return Err(Error::new("workspace", "Workspace must exist"));
    }
    validation::reject_link(root)?;
    let action = field(&request, "action")?;
    match action {
        "publish" => {
            let mut options = request
                .as_object()
                .cloned()
                .ok_or_else(|| Error::new("input", "Publication options must be an object"))?;
            options.remove("action");
            return crate::publish::publish_with_options(
                root,
                &serde_json::from_value(Value::Object(options))?,
            );
        }
        "job-list" => return crate::jobs::list(root),
        "job-status" => return crate::jobs::status(root, field(&request, "job")?),
        "job-cancel" => return crate::jobs::cancel(root, field(&request, "job")?),
        "job-retry" => return crate::jobs::retry(root, field(&request, "job")?),
        "job-run" => {
            return crate::jobs::run(
                root,
                request
                    .get("request")
                    .cloned()
                    .ok_or_else(|| Error::new("input", "Supply job request"))?,
                request["timeout"].as_u64().unwrap_or(120),
                request["job"].as_str(),
            );
        }
        _ => {}
    }
    crate::jobs::checkpoint()?;
    let _writer = transaction::Writer::acquire(root)?;
    transaction::recover(root)?;
    let r = registry::load(root)?;
    match action {
        "init" => {
            registry::save(root, &r)?;
            let lock = registry::lock(root)?;
            registry::save_lock(root, &lock)?;
            Ok(json!({"initialized":root,"sources":r.sources.len()}))
        }
        "status" => Ok(
            json!({"version":env!("CARGO_PKG_VERSION"),"sources":r.sources.len(),"packages":r.packages.len(),"installed":install::receipts(root)?.len(),"workspace":root}),
        ),
        "source-list" => Ok(serde_json::to_value(r.sources)?),
        "source-add" => {
            let source: Source = serde_json::from_value(
                json!({"id":field(&request,"id")?,"repository":field(&request,"repository")?,"enabled":request["enabled"].as_bool().unwrap_or(false),"roots":request.get("roots").cloned().unwrap_or(json!([])),"targets":request.get("targets").cloned().unwrap_or(json!([])),"reference":request.get("reference").cloned().unwrap_or(json!({"strategy":"default-branch"})),"excludes":request.get("excludes").cloned().unwrap_or(json!([])),"update":request.get("update").cloned().unwrap_or(json!("review")),"metadata":request.get("metadata").cloned().unwrap_or(json!({})),"credential_reference":request.get("credential_reference").cloned().unwrap_or(Value::Null)}),
            )?;
            registry::add_source(root, source)?;
            Ok(json!({"registered":request["id"],"approval_required":true}))
        }
        "source-inspect" => {
            let id = field(&request, "source")?;
            let s = r
                .sources
                .iter()
                .find(|s| s.id == id)
                .ok_or_else(|| Error::new("source", "Unknown source"))?;
            Ok(serde_json::to_value(s)?)
        }
        "source-policy" => {
            let mut r = r;
            let id = field(&request, "source")?;
            let s = r
                .sources
                .iter_mut()
                .find(|s| s.id == id)
                .ok_or_else(|| Error::new("source", "Unknown source"))?;
            if let Some(enabled) = request["enabled"].as_bool() {
                s.enabled = enabled;
            }
            if let Some(policy) = request.get("update") {
                s.update = serde_json::from_value(policy.clone())?;
            }
            registry::save(root, &r)?;
            Ok(json!({"updated":id}))
        }
        "discover" => sync::discover(
            root,
            field(&request, "source")?,
            request["offline"].as_bool().unwrap_or(false),
        ),
        "approve" => sync::approve(
            root,
            field(&request, "source")?,
            field(&request, "digest")?,
            request["allow_executable"].as_bool().unwrap_or(false),
        ),
        "catalog" => {
            let mut packages = r
                .packages
                .iter()
                .map(serde_json::to_value)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            packages.extend(crate::collection_skills::catalog(root)?);
            Ok(json!(packages))
        }
        "package-add" => {
            let p: Package = serde_json::from_value(
                request
                    .get("package")
                    .cloned()
                    .ok_or_else(|| Error::new("package", "Supply package registration"))?,
            )?;
            registry::add_package(root, p)?;
            Ok(json!({"registered":true}))
        }
        "package-inspect" => {
            let id = field(&request, "package")?;
            if let Some(accepted) = crate::collection_skills::find(root, id)? {
                crate::collections::verify_selected(root, Some(id))?;
                return Ok(serde_json::to_value(accepted)?);
            }
            Ok(serde_json::to_value(
                r.packages
                    .iter()
                    .find(|p| p.id == id)
                    .ok_or_else(|| Error::new("package", "Unknown package"))?,
            )?)
        }
        "harnesses" => Ok(serde_json::to_value(adapters::capabilities())?),
        "compatibility" => {
            let id = field(&request, "package")?;
            let target = field(&request, "target")?;
            if let Some(accepted) = crate::collection_skills::find(root, id)? {
                crate::collections::verify_selected(root, Some(id))?;
                return crate::collection_skills::assess(root, &accepted, target);
            }
            let lock = registry::lock(root)?;
            let closure = crate::build_dependencies::resolve(&r, &lock, id, target)?;
            for (package, accepted) in &closure {
                if package.components.iter().any(|selected| {
                    !accepted
                        .snapshot
                        .components
                        .iter()
                        .any(|component| &component.id == selected)
                }) {
                    return Err(Error::new(
                        "component-selection",
                        "Selected component is absent from accepted inventory",
                    ));
                }
            }
            let components = closure
                .iter()
                .flat_map(|(p, a)| {
                    a.snapshot
                        .components
                        .iter()
                        .filter(|c| {
                            (p.components.is_empty() || p.components.contains(&c.id))
                                && !(!c.required && p.optional_exclusions.contains(&c.id))
                        })
                        .cloned()
                })
                .collect::<Vec<_>>();
            adapters::assess(&components, target)
        }
        "collection-sync" => crate::collections::sync(
            root,
            request["apply"].as_bool().unwrap_or(false),
            request["offline"].as_bool().unwrap_or(false),
        ),
        "collection-design-list" => crate::collections::design_list(
            root,
            field(&request, "collection")?,
            request["offline"].as_bool().unwrap_or(false),
        ),
        "collection-design-review" => crate::collections::design_review(
            root,
            field(&request, "collection")?,
            field(&request, "root")?,
            request["revision"].as_str(),
            request["offline"].as_bool().unwrap_or(false),
        ),
        "collection-design-accept" => crate::collections::design_accept(
            root,
            field(&request, "collection")?,
            field(&request, "root")?,
            field(&request, "revision")?,
            field(&request, "digest")?,
            request["offline"].as_bool().unwrap_or(false),
        ),
        "collection-verify" => crate::collections::verify(root),
        "sync" => sync::sync(
            root,
            request["apply"].as_bool().unwrap_or(false),
            request["offline"].as_bool().unwrap_or(false),
        ),
        "build" => build::build(
            root,
            field(&request, "package")?,
            field(&request, "target")?,
        ),
        "validate" => {
            registry::validate(&r)?;
            Ok(
                json!({"registry_valid":true,"packages":r.packages.len(),"validation":"static-only"}),
            )
        }
        "installed" => Ok(serde_json::to_value(install::receipts(root)?)?),
        "install" => {
            let destination = PathBuf::from(field(&request, "destination")?);
            if !destination.is_absolute() {
                return Err(Error::new("destination", "Select an absolute destination"));
            }
            let scope = request.get("scope").cloned().unwrap_or(json!("project"));
            install::install_scoped_approved(
                root,
                field(&request, "package")?,
                field(&request, "target")?,
                &destination,
                serde_json::from_value(scope)?,
                request["dry_run"].as_bool().unwrap_or(true),
                request["native_approval"].as_str(),
            )
        }
        "disable" => install::disable(root, field(&request, "receipt")?),
        "enable" => install::enable(root, field(&request, "receipt")?),
        "uninstall" => install::uninstall(root, field(&request, "receipt")?),
        "rollback" => {
            transaction::rollback(root, field(&request, "operation")?)?;
            Ok(json!({"rolled_back":request["operation"]}))
        }
        "history" => list_json(&root.join(".forge-state/history")),
        "support-bundle" => {
            crate::support::bundle(root, request["include_logs"].as_bool().unwrap_or(false))
        }
        "log-retention" => crate::support::retention(
            root,
            request["days"].as_u64().unwrap_or(30),
            request["keep"]
                .as_u64()
                .unwrap_or(10)
                .try_into()
                .map_err(|_| Error::new("retention", "Invalid retention count"))?,
            request["apply"].as_bool().unwrap_or(false),
        ),
        "logs" => {
            let p = root.join(".forge-state/logs");
            let mut entries = vec![];
            if p.exists() {
                for e in fs::read_dir(p)? {
                    entries.push(e?.path().display().to_string());
                }
            }
            Ok(json!({"files":entries}))
        }
        "settings" => Ok(
            json!({"publication":"preview","concurrency":1,"telemetry":false,"executable_trust":"explicit","locale":"en"}),
        ),
        "doctor" => {
            let git = crate::process::text(&["--version"]);
            Ok(
                json!({"git":git.ok(),"workspace_writable":!fs::metadata(root)?.permissions().readonly(),"host_smoke_tests":"not-performed"}),
            )
        }
        "update" => {
            let mut results = Vec::new();
            let mut partial = false;
            let receipts = install::receipts(root)?;
            for receipt in receipts.iter().filter(|receipt| {
                receipt.enabled && crate::jobs::selected("receipt", &receipt.id, None)
            }) {
                crate::jobs::progress("receipt", &receipt.id, None, "pending")?;
            }
            let mut destinations: Vec<PathBuf> = Vec::new();
            for receipt in receipts.iter().filter(|receipt| {
                receipt.enabled && crate::jobs::selected("receipt", &receipt.id, None)
            }) {
                if !destinations
                    .iter()
                    .any(|destination| install::same_destination(destination, &receipt.destination))
                {
                    destinations.push(receipt.destination.clone());
                }
            }
            for destination in destinations {
                let group = receipts
                    .iter()
                    .filter(|receipt| {
                        receipt.enabled
                            && install::same_destination(&receipt.destination, &destination)
                    })
                    .collect::<Vec<_>>();
                crate::jobs::checkpoint()?;
                for receipt in &group {
                    crate::jobs::progress("receipt", &receipt.id, None, "running")?;
                }
                let outcome = (|| -> Result<Value> {
                    let mut built = std::collections::BTreeSet::new();
                    let mut verified_collection = false;
                    for receipt in &group {
                        if !built.insert((&receipt.package, &receipt.target)) {
                            continue;
                        }
                        if r.packages.iter().any(|p| p.id == receipt.package) {
                            build::build(root, &receipt.package, &receipt.target)?;
                        } else if !verified_collection {
                            crate::collections::verify(root)?;
                            verified_collection = true;
                        }
                    }
                    install::update_installed(root, &destination, false)
                })();
                match outcome {
                    Ok(value) => {
                        for receipt in &group {
                            crate::jobs::progress("receipt", &receipt.id, None, "success")?;
                        }
                        results.push(value);
                    }
                    Err(error)
                        if matches!(error.code, "recovery-required" | "cancelled" | "timeout") =>
                    {
                        return Err(error);
                    }
                    Err(error) => {
                        partial = true;
                        for receipt in &group {
                            crate::jobs::progress("receipt", &receipt.id, None, "failed")?;
                        }
                        results.push(json!({"destination":destination,"status":"failed","code":error.code,"message":crate::logging::redact(&error.message)}));
                    }
                }
            }
            Ok(json!({"status":if partial {"partial"} else {"success"},"installed":results}))
        }
        _ => Err(Error::new("action", "Unknown application operation")),
    }
}
fn field<'a>(r: &'a Value, name: &str) -> Result<&'a str> {
    r[name]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::new("input", format!("Missing string field: {name}")))
}
fn list_json(directory: &Path) -> Result<Value> {
    let mut rows = vec![];
    if directory.exists() {
        for entry in fs::read_dir(directory)? {
            let p = entry?.path();
            if p.extension().and_then(|s| s.to_str()) == Some("json") {
                rows.push(storage::read::<Value>(&p)?);
            }
        }
    }
    Ok(json!(rows))
}
