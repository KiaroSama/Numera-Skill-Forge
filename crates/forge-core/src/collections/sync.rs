use super::*;

pub fn execute(root: &Path, apply: bool, offline: bool) -> Result<Value> {
    let registry = load(root)?;
    let mut state = lock(root)?;
    check_catalog(root, &state)?;
    let mut statuses = vec![];
    let mut partial = false;
    for collection in &registry.collections {
        for skill in collection
            .skills
            .iter()
            .filter(|skill| crate::jobs::selected("collection", &skill.id, None))
        {
            crate::jobs::progress("collection", &skill.id, None, "pending")?;
        }
    }
    for collection in &registry.collections {
        let selected_skills = collection
            .skills
            .iter()
            .filter(|skill| crate::jobs::selected("collection", &skill.id, None))
            .collect::<Vec<_>>();
        if selected_skills.is_empty() {
            continue;
        }
        crate::jobs::checkpoint()?;
        let repository = match Repository::open(root, collection, offline) {
            Ok(repo) => repo,
            Err(e) => {
                if matches!(e.code, "cancelled" | "timeout") {
                    return Err(e);
                }
                partial = true;
                for skill in &selected_skills {
                    crate::jobs::progress("collection", &skill.id, None, "failed")?;
                }
                statuses.push(json!({"collection":collection.id,"status":"failed","code":e.code}));
                continue;
            }
        };
        let revision = match repository.revision(&format!("refs/heads/{}", collection.branch)) {
            Ok(sha) => sha,
            Err(e) => {
                if matches!(e.code, "cancelled" | "timeout") {
                    return Err(e);
                }
                partial = true;
                for skill in &selected_skills {
                    crate::jobs::progress("collection", &skill.id, None, "failed")?;
                }
                statuses.push(json!({"collection":collection.id,"status":"failed","code":e.code}));
                continue;
            }
        };
        let tree = repository.tree(&revision)?;
        for skill in selected_skills {
            crate::jobs::checkpoint()?;
            crate::jobs::progress("collection", &skill.id, None, "running")?;
            let key = format!("{}/{}", collection.id, skill.id);
            let mut prior = state.entries.get(&key).cloned();
            let outcome = (|| -> Result<&'static str> {
                if apply
                    && prior.as_ref().is_some_and(|a| a.delivery.is_empty())
                    && root.join("packages/claude-code").join(&skill.id).exists()
                {
                    delivery::prepare(root, &mut state, &skill.id)?;
                    prior = state.entries.get(&key).cloned();
                }
                if let Some(previous) = &prior
                    && apply
                    && !root
                        .join("packages/claude-code")
                        .join(&previous.id)
                        .exists()
                {
                    let previous = previous.clone();
                    repository.ensure_revision(&previous.revision, offline)?;
                    let old_selected = selection(&repository.tree(&previous.revision)?, skill)?;
                    let old_payload = repository.blobs(&old_selected)?;
                    policy::verify_license(root, skill, &old_payload)?;
                    let old_files = files(skill, &old_selected, &old_payload)?;
                    if old_files != previous.files {
                        return Err(Error::new(
                            "integrity",
                            "Accepted snapshot differs during reconstruction",
                        ));
                    }
                    let (mut rebuilt, mut outputs) = package(
                        root,
                        collection,
                        skill,
                        &previous.revision,
                        previous.version.clone(),
                        old_files,
                        &old_payload,
                    )?;
                    if rebuilt.generated != previous.generated {
                        return Err(Error::new(
                            "integrity",
                            "Accepted plugin differs during reconstruction",
                        ));
                    }
                    let metadata = delivery::descriptors(&rebuilt)?;
                    rebuilt.delivery = metadata
                        .iter()
                        .map(|(p, b)| (p.clone(), storage::digest(b)))
                        .collect();
                    outputs.extend(metadata);
                    let mut restored = Lock {
                        schema_version: 1,
                        entries: state.entries.clone(),
                    };
                    restored.entries.insert(key.clone(), rebuilt.clone());
                    promote(root, &rebuilt, Some(&previous), outputs, &restored)?;
                    state = restored;
                    prior = Some(rebuilt);
                }
                let initial_tree;
                let accepted_revision = if prior.is_none() {
                    &collection.reviewed_revision
                } else {
                    &revision
                };
                let selected_tree = if prior.is_none() && accepted_revision != &revision {
                    repository.ensure_revision(accepted_revision, offline)?;
                    initial_tree = repository.tree(accepted_revision)?;
                    &initial_tree
                } else {
                    &tree
                };
                let selected = selection(selected_tree, skill)?;
                let candidate = repository.blobs(&selected)?;
                policy::verify_license(root, skill, &candidate)?;
                let candidate_files = files(skill, &selected, &candidate)?;
                let digest = storage::digest(&storage::encoded(&candidate_files)?);
                if let Some(prior) = &prior {
                    design::verify(root, skill, prior)?;
                    check_owned(root, state.entries.get(&key).unwrap_or(prior))?;
                    adaptation::verify(root, skill, prior)?;
                    if !skill.notices.is_empty() {
                        for (index, notice) in skill.notices.iter().enumerate() {
                            let path = format!(
                                "packages/claude-code/{}/upstream-notices/original-{index}.txt",
                                skill.id
                            );
                            if prior.generated.get(&path) != Some(&notice.digest) {
                                return Ok("review-required");
                            }
                        }
                        let attribution = storage::encoded(
                            &json!({"sources":skill.notices,"attribution":skill.attribution,"payload":adaptation::payload_claim(skill)}),
                        )?;
                        let path = format!(
                            "packages/claude-code/{}/upstream-notices/attribution.json",
                            skill.id
                        );
                        if prior.generated.get(&path) != Some(&storage::digest(&attribution)) {
                            return Ok("review-required");
                        }
                    }
                    if prior.content_digest == digest {
                        return Ok("unchanged");
                    }
                    repository.ensure_revision(&prior.revision, offline)?;
                    let old_blobs = selection(&repository.tree(&prior.revision)?, skill)?;
                    let old_payload = repository.blobs(&old_blobs)?;
                    if !policy::safe_update(prior, &candidate_files, &old_payload, &candidate) {
                        return Ok("review-required");
                    }
                }
                if !apply {
                    return Ok("update-available");
                }
                let version = if let Some(prior) = &prior {
                    let patch = prior
                        .version
                        .rsplit('.')
                        .next()
                        .and_then(|s| s.parse::<u64>().ok())
                        .ok_or_else(|| Error::new("version", "Invalid collection version"))?;
                    format!("1.0.{}", patch + 1)
                } else {
                    "1.0.0".into()
                };
                let (mut accepted, mut outputs) = package(
                    root,
                    collection,
                    skill,
                    accepted_revision,
                    version,
                    candidate_files,
                    &candidate,
                )?;
                let metadata = delivery::descriptors(&accepted)?;
                accepted.delivery = metadata
                    .iter()
                    .map(|(p, b)| (p.clone(), storage::digest(b)))
                    .collect();
                outputs.extend(metadata);
                let mut next = Lock {
                    schema_version: 1,
                    entries: state.entries.clone(),
                };
                next.entries.insert(key.clone(), accepted.clone());
                promote(root, &accepted, prior.as_ref(), outputs, &next)?;
                state = next;
                Ok("updated")
            })();
            match outcome {
                Ok(status) => {
                    partial |= status == "review-required";
                    crate::jobs::progress(
                        "collection",
                        &skill.id,
                        None,
                        if status == "review-required" {
                            status
                        } else {
                            "success"
                        },
                    )?;
                    statuses.push(json!({"collection":collection.id,"skill":skill.id,"status":status,"revision":revision}));
                }
                Err(e) => {
                    if matches!(e.code, "recovery-required" | "cancelled" | "timeout") {
                        return Err(e);
                    }
                    partial = true;
                    crate::jobs::progress("collection", &skill.id, None, "failed")?;
                    statuses.push(json!({"collection":collection.id,"skill":skill.id,"status":"failed","code":e.code,"message":e.message}));
                }
            }
        }
    }
    if apply {
        catalog(root, &state)?;
    }
    Ok(json!({"status":if partial {"partial"} else {"success"},"concurrency":1,"skills":statuses}))
}
