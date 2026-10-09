use crate::{
    Error, Result,
    model::{Accepted, UpdatePolicy},
    registry, source, storage,
};
use serde_json::{Value, json};
use std::path::Path;

pub fn discover(root: &Path, id: &str, offline: bool) -> Result<Value> {
    let registry = registry::load(root)?;
    let source = registry
        .sources
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| Error::new("source", "Unknown source"))?;
    let (snapshot, _) = source::candidate(root, source, offline)?;
    storage::write(
        &root
            .join(".forge-state/candidates")
            .join(format!("{id}.json")),
        &snapshot,
    )?;
    Ok(serde_json::to_value(snapshot)?)
}
pub fn approve(root: &Path, id: &str, digest: &str, allow_executable: bool) -> Result<Value> {
    crate::validation::identifier(id)?;
    let snapshot: crate::model::Snapshot = storage::read(
        &root
            .join(".forge-state/candidates")
            .join(format!("{id}.json")),
    )?;
    if snapshot.content_digest != digest {
        return Err(Error::new(
            "approval-stale",
            "Candidate differs from the explicitly reviewed digest",
        ));
    }
    let executable = snapshot.components.iter().any(|c| c.kind.executable())
        || snapshot.files.iter().any(|f| f.executable);
    if executable && !allow_executable {
        return Err(Error::new(
            "trust-required",
            "Candidate contains executable content; explicit executable approval required",
        ));
    }
    let mut lock = registry::lock(root)?;
    lock.sources.insert(
        id.into(),
        Accepted {
            snapshot,
            approved_digest: digest.into(),
            executable_approved: allow_executable,
        },
    );
    registry::save_lock(root, &lock)?;
    Ok(json!({"approved":id,"digest":digest}))
}
pub fn sync(root: &Path, apply: bool, offline: bool) -> Result<Value> {
    let registry = registry::load(root)?;
    let mut lock = registry::lock(root)?;
    let mut results = vec![];
    let mut partial = false;
    for source in registry.sources.iter().filter(|s| s.enabled) {
        match source::candidate(root, source, offline) {
            Err(e) => {
                partial = true;
                results.push(json!({"source":source.id,"status":"failed","code":e.code,"message":e.message,"retained":lock.sources.contains_key(&source.id)}));
            }
            Ok((snapshot, _)) => {
                storage::write(
                    &root
                        .join(".forge-state/candidates")
                        .join(format!("{}.json", source.id)),
                    &snapshot,
                )?;
                let previous = lock.sources.get(&source.id);
                let unchanged = previous.is_some_and(|p| {
                    p.snapshot.content_digest == snapshot.content_digest
                        && p.snapshot.license_digest == snapshot.license_digest
                });
                storage::write(
                    &root
                        .join(".forge-state/observed")
                        .join(format!("{}.json", source.id)),
                    &json!({"revision":snapshot.revision}),
                )?;
                if unchanged {
                    results.push(json!({"source":source.id,"status":"unchanged","observed":snapshot.revision}));
                    continue;
                }
                let executable = snapshot.files.iter().any(|f| f.executable)
                    || snapshot.components.iter().any(|c| c.kind.executable());
                let policy_allows = matches!(source.update, UpdatePolicy::ApprovedContentOnly)
                    && previous.is_some()
                    && !executable
                    && previous.is_some_and(|p| {
                        p.snapshot.license_digest == snapshot.license_digest
                            && p.snapshot
                                .components
                                .iter()
                                .map(|c| &c.id)
                                .eq(snapshot.components.iter().map(|c| &c.id))
                    });
                if apply && policy_allows {
                    lock.sources.insert(
                        source.id.clone(),
                        Accepted {
                            approved_digest: snapshot.content_digest.clone(),
                            snapshot,
                            executable_approved: false,
                        },
                    );
                    results.push(json!({"source":source.id,"status":"accepted"}));
                } else {
                    partial = true;
                    results.push(json!({"source":source.id,"status":"review-required","digest":snapshot.content_digest,"revision":snapshot.revision,"executable":executable}));
                }
            }
        }
    }
    if apply {
        registry::save_lock(root, &lock)?;
    }
    Ok(json!({"status":if partial{"partial"}else{"success"},"concurrency":1,"sources":results}))
}
