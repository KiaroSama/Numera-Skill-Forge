use crate::{
    Error, Result, adapters, build, storage,
    transaction::{self, Change},
    validation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, Clone)]
pub struct Receipt {
    pub id: String,
    pub package: String,
    pub target: String,
    pub destination: PathBuf,
    pub files: BTreeMap<String, String>,
    pub enabled: bool,
    pub operation: String,
}
pub fn receipts(root: &Path) -> Result<Vec<Receipt>> {
    let directory = root.join(".forge-state/receipts");
    if !directory.exists() {
        return Ok(vec![]);
    }
    fs::read_dir(directory)?
        .map(|e| storage::read(&e?.path()))
        .collect()
}
fn receipt_path(root: &Path, id: &str) -> Result<PathBuf> {
    validation::identifier(id)?;
    Ok(root
        .join(".forge-state/receipts")
        .join(format!("{id}.json")))
}
pub fn install(
    root: &Path,
    id: &str,
    target: &str,
    destination: &Path,
    dry: bool,
) -> Result<Value> {
    let package = build::package_root(root, id, target)?;
    let skill_root = package.join("skills");
    if !skill_root.is_dir() {
        return Err(Error::new(
            "build-required",
            "Build this target package first",
        ));
    }
    validation::reject_link(destination)?;
    let prefix = adapters::install_prefix(target)?;
    let receipt_id = format!(
        "{id}-{target}-{}",
        &storage::digest(destination.to_string_lossy().as_bytes())[..12]
    );
    let previous = receipts(root)?.into_iter().find(|r| r.id == receipt_id);
    let mut changes = vec![];
    let mut owned = BTreeMap::new();
    for (p, f) in storage::files(&skill_root)? {
        let relative = format!("{prefix}/{p}");
        let path = validation::contained(destination, &relative)?;
        let after = storage::bytes(&f)?;
        let after_digest = storage::digest(&after);
        let before = if path.exists() {
            Some(storage::bytes(&path)?)
        } else {
            None
        };
        if let Some(bytes) = &before {
            let digest = storage::digest(bytes);
            let prior = previous.as_ref().and_then(|r| r.files.get(&relative));
            if prior != Some(&digest) {
                return Err(Error::new(
                    "ownership-conflict",
                    format!("Existing file is not unchanged Forge-owned material: {relative}"),
                ));
            }
        }
        owned.insert(relative.clone(), after_digest);
        if before.as_ref() != Some(&after) {
            changes.push(Change {
                path: relative,
                before,
                after: Some(after),
            });
        }
    }
    if let Some(prior) = &previous {
        for (path, digest) in &prior.files {
            if owned.contains_key(path) {
                continue;
            }
            let physical = validation::contained(destination, path)?;
            let before = storage::bytes(&physical)?;
            if storage::digest(&before) != *digest {
                return Err(Error::new(
                    "conflict",
                    "Removed package file was modified by user",
                ));
            }
            changes.push(Change {
                path: path.clone(),
                before: Some(before),
                after: None,
            });
        }
    }
    if dry {
        return Ok(
            json!({"dry_run":true,"destination":destination,"files":owned.keys().collect::<Vec<_>>(),"changes":changes.len(),"delivery":"native skill installation, not runtime plugin enablement"}),
        );
    }
    let operation = transaction::apply(root, destination, changes)?;
    let receipt = Receipt {
        id: receipt_id.clone(),
        package: id.into(),
        target: target.into(),
        destination: destination.into(),
        files: owned,
        enabled: true,
        operation,
    };
    storage::write(&receipt_path(root, &receipt_id)?, &receipt)?;
    Ok(serde_json::to_value(receipt)?)
}
pub fn disable(root: &Path, id: &str) -> Result<Value> {
    let mut receipt: Receipt = storage::read(&receipt_path(root, id)?)?;
    if !receipt.enabled {
        return Ok(json!({"status":"unchanged"}));
    }
    let stash = root.join(".forge-state/disabled").join(id);
    fs::create_dir_all(&stash)?;
    let mut changes = vec![];
    for (path, digest) in &receipt.files {
        let physical = validation::contained(&receipt.destination, path)?;
        let bytes = storage::bytes(&physical)?;
        if storage::digest(&bytes) != *digest {
            return Err(Error::new(
                "conflict",
                "Owned file modified; disable refused",
            ));
        }
        storage::atomic(&validation::contained(&stash, path)?, &bytes)?;
        changes.push(Change {
            path: path.clone(),
            before: Some(bytes),
            after: None,
        });
    }
    receipt.operation = transaction::apply(root, &receipt.destination, changes)?;
    receipt.enabled = false;
    storage::write(&receipt_path(root, id)?, &receipt)?;
    Ok(serde_json::to_value(receipt)?)
}
pub fn enable(root: &Path, id: &str) -> Result<Value> {
    let mut receipt: Receipt = storage::read(&receipt_path(root, id)?)?;
    if receipt.enabled {
        return Ok(json!({"status":"unchanged"}));
    }
    let stash = root.join(".forge-state/disabled").join(id);
    let mut changes = vec![];
    for (path, digest) in &receipt.files {
        let physical = validation::contained(&receipt.destination, path)?;
        if physical.exists() {
            return Err(Error::new(
                "conflict",
                "Destination was changed while package disabled",
            ));
        }
        let bytes = storage::bytes(&validation::contained(&stash, path)?)?;
        if storage::digest(&bytes) != *digest {
            return Err(Error::new("integrity", "Disabled payload digest mismatch"));
        }
        changes.push(Change {
            path: path.clone(),
            before: None,
            after: Some(bytes),
        });
    }
    receipt.operation = transaction::apply(root, &receipt.destination, changes)?;
    receipt.enabled = true;
    storage::write(&receipt_path(root, id)?, &receipt)?;
    Ok(serde_json::to_value(receipt)?)
}
pub fn uninstall(root: &Path, id: &str) -> Result<Value> {
    let receipt: Receipt = storage::read(&receipt_path(root, id)?)?;
    if receipt.enabled {
        disable(root, id)?;
    }
    fs::remove_file(receipt_path(root, id)?)?;
    Ok(json!({"uninstalled":id,"durable_data_preserved":true}))
}
