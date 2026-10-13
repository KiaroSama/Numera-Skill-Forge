use crate::{
    Error, Result, adapters,
    model::BuildIdentity,
    registry, storage,
    transaction::{self, Change},
    validation,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        if self.0.exists() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

pub fn build(workspace: &Path, id: &str, target: &str) -> Result<Value> {
    validation::identifier(id)?;
    validation::identifier(target)?;
    let r = registry::load(workspace)?;
    if crate::collection_skills::find(workspace, id)?.is_some() {
        return crate::collections::build(workspace, id, target);
    }
    let package = r
        .packages
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| Error::new("package", "Unknown package"))?;
    let mut lock = registry::lock(workspace)?;
    let closure = crate::build_dependencies::resolve(&r, &lock, id, target)?;
    let mut selected_components = Vec::new();
    for (dependency, accepted) in &closure {
        crate::jobs::checkpoint()?;
        let snapshot = &accepted.snapshot;
        for selected in &dependency.components {
            if !snapshot
                .components
                .iter()
                .any(|component| &component.id == selected)
            {
                return Err(Error::new(
                    "component-selection",
                    "Package references a component absent from the accepted inventory",
                ));
            }
        }
        if (snapshot.files.iter().any(crate::model::runtime_payload)
            || snapshot.components.iter().any(|c| c.kind.executable()))
            && !accepted.executable_approved
        {
            return Err(Error::new(
                "trust-required",
                "Executable companions require explicit source approval",
            ));
        }
        for component in snapshot
            .components
            .iter()
            .filter(|c| dependency.components.is_empty() || dependency.components.contains(&c.id))
        {
            if !component.required && dependency.optional_exclusions.contains(&component.id) {
                continue;
            }
            if !selected_components.iter().any(
                |(_, _, existing): &(
                    &crate::model::Package,
                    &crate::model::Snapshot,
                    crate::model::Component,
                )| existing.id == component.id,
            ) {
                selected_components.push((*dependency, snapshot, component.clone()));
            }
        }
    }
    let components = selected_components
        .iter()
        .map(|(_, _, c)| c.clone())
        .collect::<Vec<_>>();
    if components.is_empty() {
        return Err(Error::new("empty-package", "No selected components"));
    }
    let mut report = adapters::assess(&components, target)?;
    report["components"] = serde_json::to_value(&components)?;
    if report["supported"] != true {
        return Err(Error::new(
            "unsupported-required",
            serde_json::to_string(&report)?,
        ));
    }
    let input_digest = storage::digest(&storage::encoded(&(
        &closure
            .iter()
            .map(|(p, accepted)| (*p, &accepted.snapshot.content_digest))
            .collect::<Vec<_>>(),
        target,
        "adapter-v3",
    ))?);
    let key = format!("{target}/{id}");
    let prior = lock.builds.get(&key);
    #[cfg(unix)]
    if prior.is_some()
        && workspace
            .join("packages")
            .join(target)
            .join(id)
            .join("provenance.json")
            .exists()
    {
        use std::os::unix::fs::PermissionsExt;
        let prior_root = workspace.join("packages").join(target).join(id);
        let provenance: Value = storage::read(&prior_root.join("provenance.json"))?;
        if let Some(modes) = provenance["file_modes"].as_object() {
            for (path, mode) in modes {
                let expected = mode
                    .as_u64()
                    .ok_or_else(|| Error::new("integrity", "Invalid owned mode metadata"))?;
                if fs::metadata(validation::contained(&prior_root, path)?)?
                    .permissions()
                    .mode() as u64
                    & 0o777
                    != expected
                {
                    return Err(Error::new(
                        "generated-conflict",
                        "Owned generated permissions changed; user changes preserved",
                    ));
                }
            }
        }
    }
    let existing = workspace.join("packages").join(target).join(id);
    if existing.exists() && !storage::payload_files(&existing)?.is_empty() {
        match prior {
            Some(identity) if tree_digest(&existing)? == identity.payload_digest => {}
            _ => {
                return Err(Error::new(
                    "generated-conflict",
                    "Generated package was modified or is not Forge-owned; rebuild preserves it and requires review",
                ));
            }
        }
    }
    let version = match prior {
        Some(prior) if prior.input_digest == input_digest => prior.version.clone(),
        Some(prior) => {
            let patch = prior
                .version
                .rsplit('.')
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .ok_or_else(|| Error::new("version", "Invalid generated version"))?;
            format!("0.1.{}", patch + 1)
        }
        None => "0.1.0".into(),
    };
    let stage = workspace
        .join(".forge-state/build")
        .join(format!("{}-{target}", id));
    if stage.exists() {
        fs::remove_dir_all(&stage)?;
    }
    fs::create_dir_all(&stage)?;
    let _cleanup = Staging(stage.clone());
    let mut file_modes = std::collections::BTreeMap::new();
    let mut written = std::collections::BTreeMap::new();
    for (dependency, snapshot, c) in &selected_components {
        let cache = workspace
            .join(".forge-state/snapshots")
            .join(&dependency.source)
            .join(&snapshot.revision);
        let destination = match adapters::component_path(target, c) {
            Ok(path) => path,
            Err(_) if !c.required && package.optional_exclusions.contains(&c.id) => continue,
            Err(error) => return Err(error),
        };
        for file in &snapshot.files {
            let relative = if file.path == c.root {
                Some(String::new())
            } else if c.root.is_empty() {
                Some(file.path.clone())
            } else {
                file.path
                    .strip_prefix(&format!("{}/", c.root))
                    .map(str::to_owned)
            };
            if let Some(relative) = relative {
                let output = if relative.is_empty() {
                    destination.clone()
                } else if destination.is_empty() {
                    relative
                } else {
                    format!("{destination}/{relative}")
                };
                let mode = if file.executable { 0o755 } else { 0o644 };
                if let Some(prior) = written.insert(output.clone(), (file.digest.clone(), mode)) {
                    if prior != (file.digest.clone(), mode) {
                        return Err(Error::new(
                            "package-collision",
                            "Selected components write conflicting payloads",
                        ));
                    }
                    continue;
                }
                let bytes = storage::bytes(&validation::contained(&cache, &file.path)?)?;
                if storage::digest(&bytes) != file.digest {
                    return Err(Error::new(
                        "integrity",
                        "Cached payload differs from accepted digest",
                    ));
                }
                let physical = validation::contained(&stage, &output)?;
                storage::atomic(&physical, &bytes)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&physical, fs::Permissions::from_mode(mode))?;
                }
                file_modes.insert(output, mode);
            }
        }
    }
    for (dependency, accepted) in &closure {
        crate::jobs::checkpoint()?;
        let snapshot = &accepted.snapshot;
        let cache = workspace
            .join(".forge-state/snapshots")
            .join(&dependency.source)
            .join(&snapshot.revision);
        for file in &snapshot.files {
            let upper = file.path.to_ascii_uppercase();
            if upper.contains("LICENSE") || upper.contains("COPYING") || upper.contains("NOTICE") {
                let output = format!("upstream-notices/{}/{}", dependency.source, file.path);
                let bytes = storage::bytes(&validation::contained(&cache, &file.path)?)?;
                if storage::digest(&bytes) != file.digest {
                    return Err(Error::new("integrity", "Accepted notice digest differs"));
                }
                let mode = if file.executable { 0o755 } else { 0o644 };
                if written
                    .insert(output.clone(), (file.digest.clone(), mode))
                    .is_some_and(|prior| prior != (file.digest.clone(), mode))
                {
                    return Err(Error::new(
                        "package-collision",
                        "Upstream notice overlaps selected payload",
                    ));
                }
                storage::atomic(&validation::contained(&stage, &output)?, &bytes)?;
                file_modes.insert(output, mode);
            }
        }
    }
    adapters::render(&stage, id, &package.name, &version, target, &components)?;
    storage::write(
        &stage.join("provenance.json"),
        &json!({"schema_version":1,"source":package.source,"sources":closure.iter().map(|(p,a)| json!({"package":p.id,"source":p.source,"revision":a.snapshot.revision,"digest":a.snapshot.content_digest,"license_digest":a.snapshot.license_digest})).collect::<Vec<_>>(),"transformation":"adapter-v3","file_modes":file_modes}),
    )?;
    storage::write(&stage.join("compatibility.json"), &report)?;
    let destination = workspace.join("packages").join(target).join(id);
    fs::create_dir_all(&destination)?;
    #[cfg(unix)]
    let mut changes = diff_tree(&stage, &destination)?;
    #[cfg(not(unix))]
    let changes = diff_tree(&stage, &destination)?;

    let payload_digest = tree_digest(&stage)?;
    lock.builds.insert(
        key,
        BuildIdentity {
            input_digest,
            version: version.clone(),
            payload_digest: payload_digest.clone(),
        },
    );
    for (path, mode) in &file_modes {
        let physical = validation::contained(&destination, path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if physical.exists()
                && fs::metadata(&physical)?.permissions().mode() & 0o777 != *mode
                && !changes.iter().any(|c| &c.path == path)
            {
                let bytes = storage::bytes(&physical)?;
                changes.push(Change {
                    path: path.clone(),
                    before: Some(bytes.clone()),
                    after: Some(bytes),
                });
            }
        }
        let _ = (physical, mode);
    }
    let mode_changes = file_modes
        .iter()
        .filter(|(path, _)| changes.iter().any(|c| &c.path == *path))
        .map(|(path, mode)| (path.clone(), *mode))
        .collect();
    crate::jobs::checkpoint()?;
    let (mut metadata, catalog_paths) =
        crate::build_catalog::prepare(workspace, target, id, &stage, &lock)?;
    metadata.push(Change {
        path: "forge.lock.json".into(),
        before: if workspace.join("forge.lock.json").exists() {
            Some(storage::bytes(&workspace.join("forge.lock.json"))?)
        } else {
            None
        },
        after: Some(storage::encoded(&lock)?),
    });
    let operation = transaction::apply_with_receipts_modes(
        workspace,
        &destination,
        &transaction::operation_id(),
        changes,
        metadata,
        &mode_changes,
    )?;
    fs::remove_dir_all(stage)?;
    Ok(
        json!({"package":id,"target":target,"version":version,"digest":payload_digest,"operation":operation,"path":destination,"catalogs":catalog_paths}),
    )
}
pub fn tree_digest(root: &Path) -> Result<String> {
    let entries = storage::payload_files(root)?
        .into_iter()
        .map(|(p, f)| Ok((p, storage::digest(&storage::bytes(&f)?))))
        .collect::<Result<Vec<_>>>()?;
    Ok(storage::digest(&storage::encoded(&entries)?))
}
pub fn diff_tree(source: &Path, destination: &Path) -> Result<Vec<Change>> {
    let mut changes = vec![];
    let source_files = storage::payload_files(source)?;
    for (path, physical) in &source_files {
        let target = validation::contained(destination, path)?;
        let before = if target.exists() {
            Some(storage::bytes(&target)?)
        } else {
            None
        };
        let after = Some(storage::bytes(physical)?);
        if before != after {
            changes.push(Change {
                path: path.clone(),
                before,
                after,
            });
        }
    }
    for (path, physical) in storage::payload_files(destination)? {
        if !source_files.iter().any(|(p, _)| p == &path) {
            changes.push(Change {
                path,
                before: Some(storage::bytes(&physical)?),
                after: None,
            });
        }
    }
    Ok(changes)
}
pub fn package_root(workspace: &Path, id: &str, target: &str) -> Result<PathBuf> {
    validation::identifier(id)?;
    validation::identifier(target)?;
    Ok(workspace.join("packages").join(target).join(id))
}
