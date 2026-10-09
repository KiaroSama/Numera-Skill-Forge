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

pub fn build(workspace: &Path, id: &str, target: &str) -> Result<Value> {
    validation::identifier(id)?;
    validation::identifier(target)?;
    let r = registry::load(workspace)?;
    let package = r
        .packages
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| Error::new("package", "Unknown package"))?;
    let mut lock = registry::lock(workspace)?;
    let accepted = lock.sources.get(&package.source).ok_or_else(|| {
        Error::new(
            "approval-required",
            "Approve source snapshot before a locked build",
        )
    })?;
    let snapshot = &accepted.snapshot;
    let components = snapshot
        .components
        .iter()
        .filter(|c| package.components.is_empty() || package.components.contains(&c.id))
        .cloned()
        .collect::<Vec<_>>();
    if components.is_empty() {
        return Err(Error::new("empty-package", "No selected components"));
    }
    let report = adapters::assess(&components, target)?;
    if report["supported"] != true {
        return Err(Error::new(
            "unsupported-required",
            serde_json::to_string(&report)?,
        ));
    }
    let input_digest = storage::digest(&storage::encoded(&(
        package,
        &snapshot.content_digest,
        target,
        "adapter-v1",
    ))?);
    let key = format!("{target}/{id}");
    let prior = lock.builds.get(&key);
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
    let cache = workspace
        .join(".forge-state/snapshots")
        .join(&package.source)
        .join(&snapshot.revision);
    let stage = workspace
        .join(".forge-state/build")
        .join(format!("{}-{target}", id));
    if stage.exists() {
        fs::remove_dir_all(&stage)?;
    }
    fs::create_dir_all(&stage)?;
    let mut written = std::collections::BTreeMap::new();
    for c in &components {
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
                if let Some(prior) = written.insert(output.clone(), file.digest.clone()) {
                    if prior != file.digest {
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
                storage::atomic(&validation::contained(&stage, &output)?, &bytes)?;
            }
        }
    }
    for file in &snapshot.files {
        if file.path.to_uppercase().contains("LICENSE")
            || file.path.to_uppercase().contains("COPYING")
        {
            storage::atomic(
                &validation::contained(&stage, &file.path)?,
                &storage::bytes(&validation::contained(&cache, &file.path)?)?,
            )?;
        }
    }
    adapters::render(&stage, id, &package.name, &version, target, &components)?;
    storage::write(
        &stage.join("provenance.json"),
        &json!({"schema_version":1,"source":package.source,"revision":snapshot.revision,"digest":snapshot.content_digest,"license_digest":snapshot.license_digest,"transformation":"adapter-v1"}),
    )?;
    storage::write(&stage.join("compatibility.json"), &report)?;
    let destination = workspace.join("packages").join(target).join(id);
    fs::create_dir_all(&destination)?;
    let changes = diff_tree(&stage, &destination)?;
    let operation = transaction::apply(workspace, &destination, changes)?;
    let payload_digest = tree_digest(&stage)?;
    lock.builds.insert(
        key,
        BuildIdentity {
            input_digest,
            version: version.clone(),
            payload_digest: payload_digest.clone(),
        },
    );
    registry::save_lock(workspace, &lock)?;
    fs::remove_dir_all(stage)?;
    Ok(
        json!({"package":id,"target":target,"version":version,"digest":payload_digest,"operation":operation,"path":destination}),
    )
}
pub fn tree_digest(root: &Path) -> Result<String> {
    let entries = storage::files(root)?
        .into_iter()
        .map(|(p, f)| Ok((p, storage::digest(&storage::bytes(&f)?))))
        .collect::<Result<Vec<_>>>()?;
    Ok(storage::digest(&storage::encoded(&entries)?))
}
pub fn diff_tree(source: &Path, destination: &Path) -> Result<Vec<Change>> {
    let mut changes = vec![];
    let source_files = storage::files(source)?;
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
    for (path, physical) in storage::files(destination)? {
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
