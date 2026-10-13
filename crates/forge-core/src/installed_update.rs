use super::{Receipt, destination, planning, receipt_path, receipts, same_destination};
use crate::{Error, Result, config_merge, storage, transaction, validation};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// Update all enabled owners at one explicit destination in one recoverable operation.
/// The service holds the workspace writer lock and recovers interrupted journals first.
pub fn update_installed(root: &Path, selected: &Path, dry: bool) -> Result<Value> {
    let destination = destination(selected)?;
    let mut owners = receipts(root)?
        .into_iter()
        .filter(|r| same_destination(&r.destination, &destination))
        .collect::<Vec<_>>();
    owners.sort_by(|a, b| a.id.cmp(&b.id));
    let mut identities = BTreeSet::new();
    let mut before_receipts = BTreeMap::new();
    for owner in &owners {
        validation::identifier(&owner.id)?;
        if !identities.insert(owner.id.clone()) {
            return Err(Error::new(
                "integrity",
                "Duplicate installed receipt identity",
            ));
        }
        let bytes = storage::bytes(&receipt_path(root, &owner.id)?)?;
        let actual: Receipt = serde_json::from_slice(&bytes)?;
        if storage::encoded(&actual)? != storage::encoded(owner)? {
            return Err(Error::new(
                "conflict",
                "Installed receipt changed during planning",
            ));
        }
        before_receipts.insert(owner.id.clone(), bytes);
    }
    let (active, disabled): (Vec<_>, Vec<_>) = owners.into_iter().partition(|r| r.enabled);
    if active.is_empty() {
        return Ok(
            json!({"status":"unchanged", "destination":destination, "receipts":[], "dry_run":dry, "changes":0}),
        );
    }
    let operation = transaction::operation_id();
    let mut previous = active[0].clone();
    previous.files.clear();
    previous.modes.clear();
    previous.configs.clear();
    let mut union = planning::Candidate {
        receipt: previous.clone(),
        files: BTreeMap::new(),
        contributions: BTreeMap::new(),
    };
    union.receipt.operation = operation.clone();
    let mut candidates = Vec::new();
    let mut total = 0u64;
    for owner in &active {
        combine_previous(&mut previous, owner)?;
        let mut candidate = planning::candidate(
            root,
            &owner.package,
            &owner.target,
            &destination,
            owner.scope,
        )?;
        candidate.receipt.id = owner.id.clone();
        candidate.receipt.operation = operation.clone();
        let mut skill_owners = disabled.clone();
        for prior in &candidates {
            let prior: &planning::Candidate = prior;
            let mut receipt = prior.receipt.clone();
            receipt.files = prior
                .files
                .keys()
                .map(|p| (p.clone(), String::new()))
                .collect();
            skill_owners.push(receipt);
        }
        planning::check_skill_owners(&candidate.receipt, candidate.files.keys(), &skill_owners)?;
        for (path, (bytes, mode)) in &candidate.files {
            total += bytes.len() as u64;
            if total > storage::MAX_TOTAL {
                return Err(Error::new(
                    "size-limit",
                    "Coordinated installation exceeds byte limit",
                ));
            }
            if let Some((shared, shared_mode)) = union.files.get(path)
                && (shared != bytes || shared_mode != mode)
            {
                return Err(Error::new(
                    "ownership-conflict",
                    format!("Next shared payloads disagree: {path}"),
                ));
            }
            union.files.insert(path.clone(), (bytes.clone(), *mode));
        }
        for (path, (desired, _)) in &candidate.contributions {
            let value = if let Some((shared, _)) = union.contributions.get(path) {
                config_merge::combine_desired(shared, desired)?
            } else {
                desired.clone()
            };
            union.contributions.insert(path.clone(), (value, None));
        }
        candidates.push(candidate);
    }
    // Check the old and new ownership sets together: case aliases or raw/config transitions
    // must never become an ordering-dependent write to the same physical file.
    validate_paths(&previous, &union, &disabled)?;
    let (combined, changes) = planning::reconcile(union, Some(&previous), &disabled)?;
    let mut updated = Vec::new();
    for candidate in candidates {
        let mut receipt = candidate.receipt;
        for path in candidate.files.keys() {
            receipt
                .files
                .insert(path.clone(), combined.files[path].clone());
            receipt.modes.insert(path.clone(), combined.modes[path]);
        }
        for (path, (desired, _)) in candidate.contributions {
            receipt.configs.insert(
                path.clone(),
                config_merge::select_fields(&combined.configs[&path], &desired)?,
            );
        }
        updated.push(receipt);
    }
    let result = json!({"status":"updated", "operation":operation, "destination":destination,
        "dry_run":dry, "changes":changes.len(), "receipts":updated});
    if dry {
        return Ok(result);
    }
    let receipt_changes = updated
        .iter()
        .map(|r| {
            Ok(transaction::Change {
                path: format!(".forge-state/receipts/{}.json", r.id),
                before: before_receipts.remove(&r.id),
                after: Some(storage::encoded(r)?),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    transaction::apply_with_receipts_modes(
        root,
        &destination,
        &operation,
        changes,
        receipt_changes,
        &combined.modes,
    )?;
    Ok(result)
}

fn combine_previous(union: &mut Receipt, owner: &Receipt) -> Result<()> {
    for (path, digest) in &owner.files {
        if union.files.get(path).is_some_and(|prior| prior != digest)
            || union
                .modes
                .get(path)
                .zip(owner.modes.get(path))
                .is_some_and(|(a, b)| a != b)
        {
            return Err(Error::new(
                "ownership-conflict",
                "Installed shared ownership disagrees",
            ));
        }
        union.files.insert(path.clone(), digest.clone());
        if let Some(mode) = owner.modes.get(path) {
            union.modes.insert(path.clone(), *mode);
        }
    }
    for (path, config) in &owner.configs {
        if let Some(shared) = union.configs.get_mut(path) {
            config_merge::combine_owned(shared, config)?;
        } else {
            union.configs.insert(path.clone(), config.clone());
        }
    }
    Ok(())
}

fn validate_paths(
    previous: &Receipt,
    next: &planning::Candidate,
    disabled: &[Receipt],
) -> Result<()> {
    let mut paths = BTreeMap::new();
    let raw = previous
        .files
        .keys()
        .chain(next.files.keys())
        .chain(disabled.iter().flat_map(|r| r.files.keys()));
    let config = previous
        .configs
        .keys()
        .chain(next.contributions.keys())
        .chain(disabled.iter().flat_map(|r| r.configs.keys()));
    for (path, kind) in raw.map(|p| (p, false)).chain(config.map(|p| (p, true))) {
        validation::relative_path(path)?;
        let key = path.to_lowercase();
        if let Some((prior, prior_kind)) = paths.get(&key) {
            if prior != path || *prior_kind != kind {
                return Err(Error::new(
                    "ownership-conflict",
                    "Case aliases or raw/config ownership overlap",
                ));
            }
        } else {
            paths.insert(key, (path.clone(), kind));
        }
    }
    for path in paths.keys() {
        for (separator, _) in path.match_indices('/') {
            if paths.contains_key(&path[..separator]) {
                return Err(Error::new(
                    "ownership-conflict",
                    "Owned file overlaps a payload directory",
                ));
            }
        }
    }
    Ok(())
}
