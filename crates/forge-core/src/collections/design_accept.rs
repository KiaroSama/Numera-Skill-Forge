use super::*;

pub fn accept(
    root: &Path,
    id: &str,
    source_root: &str,
    revision: &str,
    digest: &str,
    offline: bool,
) -> Result<Value> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::new(
            "design-approval",
            "Supply the exact reviewed candidate digest",
        ));
    }
    if revision.len() != 40 || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::new(
            "revision",
            "Supply an immutable reviewed revision",
        ));
    }
    let receipt_path = format!(".forge-state/collections/design-approvals/{digest}.json");
    let receipt_file = validation::contained(root, &receipt_path)?;
    if receipt_file.exists() {
        let receipt: Value = storage::read(&receipt_file)?;
        if receipt["collection"] != id
            || receipt["root"] != source_root
            || receipt["revision"] != revision
            || receipt["registry_digest"]
                != storage::digest(&storage::bytes(&root.join("registry/collections.json"))?)
            || receipt["lock_digest"] != storage::digest(&storage::encoded(&lock(root)?)?)
        {
            return Err(Error::new(
                "approval-stale",
                "Accepted candidate has changed since this approval",
            ));
        }
        let state = lock(root)?;
        let package = receipt["package"]
            .as_str()
            .ok_or_else(|| Error::new("approval-stale", "Approval package missing"))?;
        let accepted = state
            .entries
            .get(&format!("{id}/{package}"))
            .ok_or_else(|| Error::new("approval-stale", "Approved package missing"))?;
        if accepted.collection != id
            || accepted.root != source_root
            || accepted.revision != revision
            || receipt["digest"] != digest
        {
            return Err(Error::new(
                "approval-stale",
                "Approval does not match accepted design identity",
            ));
        }
        verify_selected(root, Some(package))?;
        return Ok(json!({"status":"unchanged","package":package,"revision":revision}));
    }
    let candidate = design_review::prepare(root, id, source_root, Some(revision), offline)?;
    if candidate.report["digest"] != digest {
        return Err(Error::new(
            "approval-stale",
            "Candidate differs from reviewed content; review again",
        ));
    }
    if candidate.report["status"] == "unchanged" {
        return Ok(
            json!({"status":"unchanged","package":candidate.accepted.id,"revision":revision}),
        );
    }
    let mut registration: Value = serde_json::from_slice(
        candidate.before["registry/collections.json"]
            .as_ref()
            .unwrap(),
    )?;
    let selected = candidate
        .registry
        .collections
        .iter()
        .find(|c| c.id == id)
        .unwrap()
        .skills
        .iter()
        .find(|s| s.root == source_root)
        .unwrap();
    let c = registration["collections"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["id"] == id)
        .unwrap();
    let skills = c["skills"].as_array_mut().unwrap();
    if let Some(old) = skills.iter_mut().find(|s| s["root"] == source_root) {
        *old = serde_json::to_value(selected)?;
    } else {
        skills.push(serde_json::to_value(selected)?);
    }
    let mut next: Lock = match candidate.before[LOCK].as_ref() {
        Some(bytes) => serde_json::from_slice(bytes)?,
        None => Lock::default(),
    };
    next.entries.insert(
        format!("{id}/{}", candidate.accepted.id),
        candidate.accepted.clone(),
    );
    let mut outputs = candidate.outputs;
    let registration_bytes = storage::encoded(&registration)?;
    outputs.insert(
        "registry/collections.json".into(),
        registration_bytes.clone(),
    );
    outputs.insert(LOCK.into(), storage::encoded(&next)?);
    for (path, value) in catalog_values(&next) {
        outputs.insert(path, storage::encoded(&value)?);
    }
    outputs.insert(receipt_path.clone(), storage::encoded(&json!({"collection":id,"root":source_root,"revision":revision,"package":candidate.accepted.id,"digest":digest,"registry_digest":storage::digest(&registration_bytes),"lock_digest":storage::digest(&storage::encoded(&next)?)}))?);
    let mut changes = vec![];
    let mut modes = vec![];
    if let Some(prior) = &candidate.previous {
        for path in prior
            .generated
            .keys()
            .chain(prior.delivery.keys())
            .filter(|p| !outputs.contains_key(*p))
        {
            changes.push(Change {
                path: path.clone(),
                before: candidate
                    .before
                    .get(path)
                    .cloned()
                    .ok_or_else(|| Error::new("integrity", "Expected owned source missing"))?,
                after: None,
            });
        }
    }
    for (path, bytes) in outputs {
        #[cfg(unix)]
        let physical = validation::contained(root, &path)?;
        let before = if path == receipt_path {
            None
        } else {
            candidate
                .before
                .get(&path)
                .cloned()
                .ok_or_else(|| Error::new("integrity", "Expected candidate output missing"))?
        };
        if path.starts_with("registry/licenses/") && before.as_ref().is_some_and(|b| b != &bytes) {
            return Err(Error::new(
                "generated-conflict",
                "Original notice destination has conflicting bytes",
            ));
        }
        if candidate.previous.is_none()
            && path.starts_with(&format!("packages/claude-code/{}/", candidate.accepted.id))
            && root
                .join(format!("packages/claude-code/{}", candidate.accepted.id))
                .exists()
        {
            return Err(Error::new(
                "generated-conflict",
                "Unowned design package destination exists",
            ));
        }
        {
            let mut old_mode = None;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if physical.exists() {
                    old_mode = Some(std::fs::metadata(&physical)?.permissions().mode() & 0o777);
                }
            }
            let _ = &mut old_mode;
            if candidate
                .accepted
                .files
                .iter()
                .any(|f| f.output_path == path)
            {
                modes.push((path.clone(), old_mode, Some(0o644)));
            }
            changes.push(Change {
                path,
                before,
                after: Some(bytes),
            });
        }
    }
    let operation = transaction::apply_modes(root, root, changes, &modes)?;
    Ok(
        json!({"status":"updated","package":candidate.accepted.id,"version":candidate.accepted.version,"revision":revision,"operation":operation,"installed_update_required":true}),
    )
}
