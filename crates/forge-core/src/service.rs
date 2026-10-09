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
    let _writer = transaction::Writer::acquire(root)?;
    transaction::recover(root)?;
    let action = field(&request, "action")?;
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
                json!({"id":field(&request,"id")?,"repository":field(&request,"repository")?,"enabled":request["enabled"].as_bool().unwrap_or(false),"roots":request.get("roots").cloned().unwrap_or(json!([])),"targets":request.get("targets").cloned().unwrap_or(json!([])),"reference":request.get("reference").cloned().unwrap_or(json!({"strategy":"default-branch"}))}),
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
        "catalog" => Ok(serde_json::to_value(r.packages)?),
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
            let p = r
                .packages
                .iter()
                .find(|p| p.id == id)
                .ok_or_else(|| Error::new("package", "Unknown package"))?;
            let lock = registry::lock(root)?;
            let s = lock.sources.get(&p.source).ok_or_else(|| {
                Error::new("approval-required", "Source has no accepted snapshot")
            })?;
            adapters::assess(&s.snapshot.components, field(&request, "target")?)
        }
        "collection-sync" => crate::collections::sync(
            root,
            request["apply"].as_bool().unwrap_or(false),
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
            install::install(
                root,
                field(&request, "package")?,
                field(&request, "target")?,
                &destination,
                request["dry_run"].as_bool().unwrap_or(true),
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
        "publish" => crate::publish::publish(
            root,
            field(&request, "mode")?,
            request["expected_remote"].as_str(),
            request["author_email"].as_str(),
        ),
        "update" => Err(Error::new(
            "update-review",
            "Build and review target package first, then reinstall with its ownership receipt",
        )),
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
