use super::*;
use model::ReviewedDesign;

pub(super) struct Candidate {
    pub report: Value,
    pub registry: Registry,
    pub previous: Option<Accepted>,
    pub accepted: Accepted,
    pub outputs: BTreeMap<String, Vec<u8>>,
    pub before: BTreeMap<String, Option<Vec<u8>>>,
}

fn collection<'a>(registry: &'a Registry, id: &str) -> Result<&'a Collection> {
    registry
        .collections
        .iter()
        .find(|c| c.id == id && c.skills.iter().any(|s| s.reviewed_design.is_some()))
        .ok_or_else(|| Error::new("design-review", "Unknown reviewed design collection"))
}

pub fn list(root: &Path, id: &str, offline: bool) -> Result<Value> {
    let registry = load(root)?;
    let c = collection(&registry, id)?;
    let repo = Repository::open(root, c, offline)?;
    let revision = repo.revision(&format!("refs/heads/{}", c.branch))?;
    let tree = repo.tree(&revision)?;
    let mut roots = BTreeSet::new();
    for blob in &tree {
        if let Some(path) = blob.path.strip_suffix("/DESIGN.md")
            && path.starts_with("design-md/")
            && path.split('/').count() == 2
        {
            roots.insert(path.to_owned());
        }
    }
    roots.extend(
        c.skills
            .iter()
            .filter(|s| s.reviewed_design.is_some())
            .map(|s| s.root.clone()),
    );
    let entries = roots.into_iter().map(|path| {
        let registered = c.skills.iter().find(|s| s.root == path);
        let present = tree.iter().any(|b| b.path == format!("{path}/DESIGN.md"));
        json!({"root":path,"package":registered.map(|s| &s.id),"status":if !present {"missing"} else if registered.is_some() {"registered"} else {"new"}})
    }).collect::<Vec<_>>();
    Ok(json!({"collection":id,"revision":revision,"designs":entries,"automatic_removal":false}))
}

pub fn review(
    root: &Path,
    id: &str,
    source_root: &str,
    revision: Option<&str>,
    offline: bool,
) -> Result<Value> {
    Ok(prepare(root, id, source_root, revision, offline)?.report)
}

pub(super) fn prepare(
    root: &Path,
    id: &str,
    source_root: &str,
    revision: Option<&str>,
    offline: bool,
) -> Result<Candidate> {
    validation::relative_path(source_root)?;
    if !source_root.starts_with("design-md/") || source_root.split('/').count() != 2 {
        return Err(Error::new(
            "design-review",
            "Select one direct design-md root",
        ));
    }
    let registry_bytes = storage::bytes(&root.join("registry/collections.json"))?;
    let mut registry: Registry = serde_json::from_slice(&registry_bytes)?;
    load(root)?;
    let state = lock(root)?;
    check_catalog(root, &state)?;
    let mut before = BTreeMap::new();
    before.insert(
        "registry/collections.json".into(),
        Some(registry_bytes.clone()),
    );
    for path in
        std::iter::once(LOCK.to_owned()).chain(catalog_values(&state).into_iter().map(|(p, _)| p))
    {
        let physical = validation::contained(root, &path)?;
        let bytes = if physical.exists() {
            Some(storage::bytes(&physical)?)
        } else {
            None
        };
        if path == LOCK {
            if bytes.as_ref().is_some_and(|b| {
                serde_json::from_slice::<Value>(b).ok() != serde_json::to_value(&state).ok()
            }) {
                return Err(Error::new(
                    "approval-stale",
                    "Collection state changed during review",
                ));
            }
        } else if let Some((_, value)) =
            catalog_values(&state).into_iter().find(|(p, _)| p == &path)
            && bytes
                .as_ref()
                .is_some_and(|b| storage::encoded(&value).ok().as_ref() != Some(b))
        {
            return Err(Error::new(
                "generated-conflict",
                "Catalog changed during review",
            ));
        }
        before.insert(path, bytes);
    }
    let c = collection(&registry, id)?;
    let repo = Repository::open(root, c, offline)?;
    let revision = if let Some(value) = revision {
        repo.ensure_revision(value, offline)?;
        repo.revision(value)?
    } else {
        repo.revision(&format!("refs/heads/{}", c.branch))?
    };
    let original = c.skills.iter().find(|s| s.root == source_root);
    if original.is_some_and(|s| s.reviewed_design.is_none()) {
        return Err(Error::new(
            "design-review",
            "Existing original skill cannot become a design adapter",
        ));
    }
    let prototype = original
        .or_else(|| c.skills.iter().find(|s| s.reviewed_design.is_some()))
        .ok_or_else(|| Error::new("design-review", "Design policy prototype missing"))?;
    let mut skill: Skill = serde_json::from_value(serde_json::to_value(prototype)?)?;
    if original.is_none() {
        let slug = source_root
            .trim_start_matches("design-md/")
            .replace('.', "-");
        skill.id = format!("skill-{}-{slug}", c.id);
        skill.root = source_root.into();
        skill.reviewed_design.as_mut().unwrap().name = format!("{}-{slug}", c.id);
        skill.reviewed_templates.clear();
        skill.reviewed_documents.clear();
        if registry
            .collections
            .iter()
            .flat_map(|c| &c.skills)
            .any(|s| {
                s.id == skill.id
                    || s.reviewed_design
                        .as_ref()
                        .is_some_and(|d| d.name == skill.reviewed_design.as_ref().unwrap().name)
            })
        {
            return Err(Error::new(
                "duplicate-id",
                "New design identity collides with an existing package",
            ));
        }
    }
    if skill.license_path.is_empty() || skill.license_path.contains('/') {
        return Err(Error::new(
            "license-review",
            "Design review requires a root license file",
        ));
    }
    let previous = state.entries.get(&format!("{id}/{}", skill.id)).cloned();
    if let Some(prior) = &previous {
        design::verify(root, original.unwrap(), prior)?;
        check_owned(root, prior)?;
        for (path, digest) in prior.generated.iter().chain(&prior.delivery) {
            let bytes = storage::bytes(&validation::contained(root, path)?)?;
            if storage::digest(&bytes) != *digest {
                return Err(Error::new(
                    "generated-conflict",
                    "Owned design changed during review",
                ));
            }
            before.insert(path.clone(), Some(bytes));
        }
    }
    let tree = repo.tree(&revision)?;
    let selected = selection(&tree, &skill)?;
    let mut payload = repo.blobs(&selected)?;
    let design_bytes = payload
        .get(&format!("{source_root}/DESIGN.md"))
        .ok_or_else(|| {
            Error::new(
                "design-review",
                "Design is missing; accepted package is retained",
            )
        })?;
    skill.reviewed_design = Some(ReviewedDesign {
        name: skill.reviewed_design.as_ref().unwrap().name.clone(),
        source_digest: storage::digest(design_bytes),
        adapter_version: 1,
    });
    skill.license_digest = storage::digest(
        payload
            .get(&skill.license_path)
            .ok_or_else(|| Error::new("license-review", "Original license is missing"))?,
    );
    let mut notices = BTreeMap::new();
    let mut scope = vec![source_root.to_owned(), skill.license_path.clone()];
    for notice in &mut skill.notices {
        let (origin, tail) = notice.source.split_once("/blob/").ok_or_else(|| {
            Error::new(
                "license-review",
                "Notice must identify an original Git file",
            )
        })?;
        let (_, path) = tail
            .split_once('/')
            .ok_or_else(|| Error::new("license-review", "Original notice path missing"))?;
        validation::relative_path(path)?;
        if path.contains('/') || policy::protected(path) {
            return Err(Error::new(
                "license-review",
                "Only original root notices may be refreshed",
            ));
        }
        let blob = tree
            .iter()
            .find(|b| b.path == path)
            .ok_or_else(|| Error::new("license-review", "Original notice was removed"))?;
        if blob.mode != "100644" {
            return Err(Error::new(
                "license-review",
                "Original notice must be a regular document",
            ));
        }
        let bytes = repo
            .blobs(std::slice::from_ref(blob))?
            .remove(path)
            .unwrap();
        std::str::from_utf8(&bytes).map_err(|_| Error::new("encoding", "Notice is not UTF-8"))?;
        let changed = notice.digest != storage::digest(&bytes);
        notice.digest = storage::digest(&bytes);
        notice.path = format!("registry/licenses/{}.txt", notice.digest);
        let source_path = path.to_owned();
        if changed {
            let repository = c.repository.trim_end_matches(".git");
            if !c.repository.starts_with("local:") && origin != repository {
                return Err(Error::new(
                    "license-review",
                    "Notice origin differs from selected collection",
                ));
            }
            notice.source = format!("{origin}/blob/{revision}/{source_path}");
        }
        scope.push(source_path);
        notices.insert(notice.path.clone(), bytes);
    }
    policy::validate_skill(&skill)?;
    for (path, bytes) in &payload {
        if selected
            .iter()
            .any(|b| &b.path == path && b.mode != "100644")
            || !path.ends_with(".md") && path != &skill.license_path && !policy::license(path)
        {
            return Err(Error::new(
                "design-review",
                "Design candidates support regular UTF-8 documents only",
            ));
        }
        std::str::from_utf8(bytes)
            .map_err(|_| Error::new("encoding", "Design document is not UTF-8"))?;
    }
    payload.extend(notices.clone());
    policy::verify_license(root, &skill, &payload)?;
    let candidate_files = files(&skill, &selected, &payload)?;
    let unchanged = previous
        .as_ref()
        .is_some_and(|p| p.files == candidate_files)
        && original
            .is_some_and(|s| serde_json::to_value(s).ok() == serde_json::to_value(&skill).ok());
    let version = if unchanged {
        previous.as_ref().unwrap().version.clone()
    } else if let Some(p) = &previous {
        let patch = p
            .version
            .strip_prefix("1.0.")
            .and_then(|s| s.parse::<u64>().ok())
            .and_then(|p| p.checked_add(1))
            .ok_or_else(|| Error::new("version", "Design version cannot advance"))?;
        format!("1.0.{patch}")
    } else {
        "1.0.0".into()
    };
    let (mut accepted, mut outputs) = package(
        root,
        c,
        &skill,
        &revision,
        version,
        candidate_files,
        &payload,
    )?;
    let descriptors = delivery::descriptors(&accepted)?;
    accepted.delivery = descriptors
        .iter()
        .map(|(p, b)| (p.clone(), storage::digest(b)))
        .collect();
    outputs.extend(descriptors);
    outputs.extend(notices);
    for (path, bytes) in &outputs {
        if !before.contains_key(path) {
            let physical = validation::contained(root, path)?;
            let existing = if physical.exists() {
                Some(storage::bytes(&physical)?)
            } else {
                None
            };
            if existing.as_ref().is_some_and(|old| old != bytes) {
                return Err(Error::new(
                    "generated-conflict",
                    "Unowned candidate output exists",
                ));
            }
            before.insert(path.clone(), existing);
        }
    }
    let baseline = previous
        .as_ref()
        .map(|p| p.revision.as_str())
        .unwrap_or(&c.reviewed_revision);
    repo.ensure_revision(baseline, offline)?;
    let mut args = vec![
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--no-renames",
        baseline,
        &revision,
        "--",
    ];
    args.extend(scope.iter().map(String::as_str));
    let diff = String::from_utf8(repo.command(&args, 30)?)
        .map_err(|_| Error::new("encoding", "Design diff is not UTF-8"))?;
    let documents = payload.iter().map(|(p,b)| json!({"path":p,"text":std::str::from_utf8(b).unwrap(),"digest":storage::digest(b)})).collect::<Vec<_>>();
    let c = registry
        .collections
        .iter_mut()
        .find(|c| c.id == id)
        .unwrap();
    if let Some(existing) = c.skills.iter_mut().find(|s| s.root == source_root) {
        *existing = skill;
    } else {
        c.skills.push(skill);
    }
    let mut report = json!({"schema_version":1,"status":if unchanged {"unchanged"} else {"review-required"},"collection":id,"root":source_root,"revision":revision,"previous_revision":previous.as_ref().map(|p| &p.revision),"registry_digest":storage::digest(&registry_bytes),"lock_digest":storage::digest(&storage::encoded(&state)?),"registration":c.skills.iter().find(|s|s.root==source_root),"files":accepted.files,"generated":accepted.generated,"delivery":accepted.delivery,"version":accepted.version,"diff":diff,"documents":documents});
    if storage::encoded(&report)?.len() > 2 * 1024 * 1024 {
        return Err(Error::new(
            "size-limit",
            "Design review exceeds 2 MiB; select a smaller source",
        ));
    }
    let digest = storage::digest(&storage::encoded(&report)?);
    report["digest"] = json!(digest);
    Ok(Candidate {
        report,
        registry,
        previous,
        accepted,
        outputs,
        before,
    })
}
