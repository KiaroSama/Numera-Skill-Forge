use crate::{
    Error, Result, config_merge,
    install::{self, Receipt},
    storage,
    transaction::{self, Change},
    validation,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

fn load(root: &Path, id: &str) -> Result<Receipt> {
    let receipt: Receipt = storage::read(&install::receipt_path(root, id)?)?;
    validation::identifier(&receipt.id)?;
    if receipt.id != id {
        return Err(Error::new(
            "integrity",
            "Receipt identity differs from selected receipt",
        ));
    }
    install::destination(&receipt.destination)?;
    Ok(receipt)
}
fn stash(root: &Path, id: &str) -> Result<std::path::PathBuf> {
    validation::identifier(id)?;
    let directory = root.join(".forge-state/disabled");
    fs::create_dir_all(&directory)?;
    validation::contained(&directory, id)
}
fn removal(receipt: &Receipt, others: &[Receipt], archive: Option<&Path>) -> Result<Vec<Change>> {
    let mut changes = vec![];
    for (path, digest) in &receipt.files {
        let physical = validation::contained(&receipt.destination, path)?;
        let bytes = install::current(&receipt.destination, path)?
            .ok_or_else(|| Error::new("conflict", "Owned file removed by user"))?;
        if storage::digest(&bytes) != *digest {
            return Err(Error::new(
                "conflict",
                "Owned file modified; lifecycle operation refused",
            ));
        }
        install::check_mode(&physical, receipt.modes.get(path))?;
        if let Some(archive) = archive {
            storage::atomic(&validation::contained(archive, path)?, &bytes)?;
        }
        if !install::shared_file(others, path, digest, receipt.modes.get(path), true)? {
            changes.push(Change {
                path: path.clone(),
                before: Some(bytes),
                after: None,
            });
        }
    }
    for (path, owned) in &receipt.configs {
        let before = install::current(&receipt.destination, path)?.ok_or_else(|| {
            Error::new("config-conflict", "Managed configuration removed by user")
        })?;
        let after = config_merge::release(&before, owned, &install::enabled_configs(others, path))?
            .map(|value| storage::encoded(&value))
            .transpose()?;
        if Some(&before) != after.as_ref() {
            changes.push(Change {
                path: path.clone(),
                before: Some(before),
                after,
            });
        }
    }
    Ok(changes)
}

pub fn disable(root: &Path, id: &str) -> Result<Value> {
    let mut receipt = load(root, id)?;
    if !receipt.enabled {
        return Ok(json!({"status":"unchanged"}));
    }
    let others = install::other_receipts(root, &receipt)?;
    let directory = stash(root, id)?;
    fs::create_dir_all(&directory)?;
    let changes = removal(&receipt, &others, Some(&directory))?;
    receipt.operation = transaction::operation_id();
    receipt.enabled = false;
    install::commit_receipt(root, &receipt, changes)?;
    Ok(serde_json::to_value(receipt)?)
}
pub fn enable(root: &Path, id: &str) -> Result<Value> {
    let mut receipt = load(root, id)?;
    if receipt.enabled {
        return Ok(json!({"status":"unchanged"}));
    }
    let directory = stash(root, id)?;
    let others = install::other_receipts(root, &receipt)?;
    let mut changes = vec![];
    for (path, digest) in &receipt.files {
        let bytes = storage::bytes(&validation::contained(&directory, path)?)?;
        if storage::digest(&bytes) != *digest {
            return Err(Error::new("integrity", "Disabled payload digest mismatch"));
        }
        let shared = install::shared_file(&others, path, digest, receipt.modes.get(path), false)?;
        let before = install::current(&receipt.destination, path)?;
        if let Some(before) = &before {
            if !shared || storage::digest(before) != *digest {
                return Err(Error::new(
                    "conflict",
                    "Destination changed while package disabled",
                ));
            }
            install::check_mode(
                &validation::contained(&receipt.destination, path)?,
                receipt.modes.get(path),
            )?;
        } else if others
            .iter()
            .any(|r| r.enabled && r.files.contains_key(path))
        {
            return Err(Error::new(
                "conflict",
                "Shared enabled payload was removed by user",
            ));
        }
        if before.is_none() {
            changes.push(Change {
                path: path.clone(),
                before,
                after: Some(bytes),
            });
        }
    }
    for (path, owned) in &mut receipt.configs {
        if others.iter().any(|r| r.files.contains_key(path)) {
            return Err(Error::new(
                "ownership-conflict",
                "Managed configuration overlaps owned raw file",
            ));
        }
        let before = install::current(&receipt.destination, path)?;
        let document = config_merge::activate(
            before.as_deref(),
            owned,
            &install::enabled_configs(&others, path),
        )?;
        let after = Some(storage::encoded(&document)?);
        if before != after {
            changes.push(Change {
                path: path.clone(),
                before,
                after,
            });
        }
    }
    receipt.operation = transaction::operation_id();
    receipt.enabled = true;
    install::commit_receipt(root, &receipt, changes)?;
    Ok(serde_json::to_value(receipt)?)
}
pub fn uninstall(root: &Path, id: &str) -> Result<Value> {
    let receipt = load(root, id)?;
    let before = storage::bytes(&install::receipt_path(root, id)?)?;
    let others = install::other_receipts(root, &receipt)?;
    let changes = if receipt.enabled {
        removal(&receipt, &others, None)?
    } else {
        vec![]
    };
    let operation = transaction::apply_with_receipt_modes(
        root,
        &receipt.destination,
        &transaction::operation_id(),
        changes,
        Change {
            path: format!(".forge-state/receipts/{id}.json"),
            before: Some(before),
            after: None,
        },
        &BTreeMap::new(),
    )?;
    Ok(json!({"uninstalled":id,"operation":operation,"durable_data_preserved":true}))
}
