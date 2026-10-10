use crate::{
    Error, Result, storage,
    transaction::{self, Change},
    validation,
};
use serde_json::{Value, json};
#[cfg(unix)]
use std::fs;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
mod acquire;
mod model;
mod policy;
use acquire::{Blob, Repository};
use model::{Accepted, Collection, File, Lock, Registry, Skill};

const LOCK: &str = "catalogs/collections.lock.json";
const MARKETPLACE: &str = ".claude-plugin/marketplace.json";

fn load(root: &Path) -> Result<Registry> {
    let path = root.join("registry/collections.json");
    let registry: Registry = if path.exists() {
        storage::read(&path)?
    } else {
        Registry {
            schema_version: 1,
            collections: vec![],
        }
    };
    if registry.schema_version != 1 {
        return Err(Error::new(
            "schema-version",
            "Unsupported collection registry",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut package_ids = BTreeSet::new();
    for collection in &registry.collections {
        validation::identifier(&collection.id)?;
        if !ids.insert(&collection.id) {
            return Err(Error::new("duplicate-id", "Collection IDs must be unique"));
        }
        if collection.branch.starts_with('-')
            || collection
                .branch
                .contains(['\0', '\r', '\n', ':', ' ', '~', '^', '?', '*', '[', '\\'])
        {
            return Err(Error::new("ref", "Invalid collection branch"));
        }
        if collection.reviewed_revision.len() != 40
            || !collection
                .reviewed_revision
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err(Error::new(
                "revision",
                "Expected reviewed immutable revision",
            ));
        }
        for skill in &collection.skills {
            policy::validate_skill(skill)?;
            if !package_ids.insert(&skill.id) {
                return Err(Error::new(
                    "duplicate-id",
                    "Collection plugin IDs must be unique",
                ));
            }
        }
    }
    Ok(registry)
}
fn lock(root: &Path) -> Result<Lock> {
    let state: Lock = if root.join(LOCK).exists() {
        storage::read(&root.join(LOCK))?
    } else {
        Lock::default()
    };
    if state.schema_version != 1 {
        return Err(Error::new("schema-version", "Unsupported collection lock"));
    }
    Ok(state)
}
fn selection(tree: &[Blob], skill: &Skill) -> Result<Vec<Blob>> {
    let selected = tree
        .iter()
        .filter(|b| {
            b.path.starts_with(&format!("{}/", skill.root))
                || b.path == skill.license_path
                || (policy::license(&b.path) && !b.path.contains('/'))
        })
        .cloned()
        .collect::<Vec<_>>();
    for file in &selected {
        if policy::protected(&file.path) && !skill.reviewed_templates.contains_key(&file.path) {
            return Err(Error::new(
                "protected-payload",
                "Skill contains excluded private/local content; no incomplete copy is published",
            ));
        }
    }
    Ok(selected)
}
fn files(
    skill: &Skill,
    selected: &[Blob],
    payload: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<File>> {
    let entry = format!("{}/SKILL.md", skill.root);
    let text = std::str::from_utf8(
        payload
            .get(&entry)
            .ok_or_else(|| Error::new("skill-entrypoint", "Selected root has no SKILL.md"))?,
    )
    .map_err(|_| Error::new("encoding", "SKILL.md is not UTF-8"))?;
    let front = text
        .strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---"))
        .ok_or_else(|| Error::new("frontmatter", "Skill requires original frontmatter"))?
        .0;
    let metadata: Value = serde_yaml_ng::from_str(front)
        .map_err(|_| Error::new("frontmatter", "Invalid original skill frontmatter"))?;
    let name = metadata["name"]
        .as_str()
        .ok_or_else(|| Error::new("frontmatter", "Original skill requires name"))?;
    validation::identifier(name)?;
    selected
        .iter()
        .map(|blob| {
            let relative = blob.path.strip_prefix(&format!("{}/", skill.root));
            let output = if let Some(path) = relative {
                format!("packages/claude-code/{}/skills/{name}/{path}", skill.id)
            } else {
                format!(
                    "packages/claude-code/{}/upstream-notices/{}",
                    skill.id,
                    blob.path.replace('/', "_")
                )
            };
            validation::relative_path(&output)?;
            let bytes = payload
                .get(&blob.path)
                .ok_or_else(|| Error::new("integrity", "Missing source blob"))?;
            Ok(File {
                path: blob.path.clone(),
                output_path: output,
                digest: storage::digest(bytes),
                mode: blob.mode.clone(),
                size: bytes.len() as u64,
            })
        })
        .collect()
}
fn package(
    root: &Path,
    collection: &Collection,
    skill: &Skill,
    revision: &str,
    version: String,
    files: Vec<File>,
    payload: &BTreeMap<String, Vec<u8>>,
) -> Result<(Accepted, BTreeMap<String, Vec<u8>>)> {
    let digest = storage::digest(&storage::encoded(&files)?);
    let mut outputs = BTreeMap::new();
    for file in &files {
        outputs.insert(file.output_path.clone(), payload[&file.path].clone());
    }
    let prefix = format!("packages/claude-code/{}", skill.id);
    for (index, notice) in skill.notices.iter().enumerate() {
        let bytes = storage::bytes(&validation::contained(root, &notice.path)?)?;
        if storage::digest(&bytes) != notice.digest {
            return Err(Error::new(
                "license-review",
                "Original notice changed during packaging",
            ));
        }
        outputs.insert(
            format!("{prefix}/upstream-notices/original-{index}.txt"),
            bytes,
        );
    }
    if !skill.notices.is_empty() {
        outputs.insert(format!("{prefix}/upstream-notices/attribution.json"), storage::encoded(&json!({"sources":skill.notices,"attribution":skill.attribution,"payload":"Exact aggregator payload; upstream adaptations were not made by Forge"}))?);
    }
    outputs.insert(format!("{prefix}/.claude-plugin/plugin.json"),storage::encoded(&json!({"name":skill.id,"version":version,"description":format!("Unmodified upstream skill: {}",skill.root),"author":{"name":"Numera Skill Forge (package assembler)"},"repository":collection.repository,"license":skill.license}))?);
    outputs.insert(format!("{prefix}/provenance.json"),storage::encoded(&json!({"schema_version":1,"collection":collection.id,"repository":collection.repository,"root":skill.root,"revision":revision,"license":skill.license,"payload_digest":digest,"files":files,"execution":"not-performed"}))?);
    let generated = outputs
        .iter()
        .map(|(path, bytes)| (path.clone(), storage::digest(bytes)))
        .collect();
    Ok((
        Accepted {
            collection: collection.id.clone(),
            id: skill.id.clone(),
            root: skill.root.clone(),
            revision: revision.into(),
            version,
            license: skill.license.clone(),
            content_digest: digest,
            files,
            generated,
        },
        outputs,
    ))
}
fn check_owned(root: &Path, accepted: &Accepted) -> Result<()> {
    let dir = root.join("packages/claude-code").join(&accepted.id);
    if !dir.exists() {
        return Ok(());
    }
    let actual = storage::payload_files(&dir)?;
    if actual.len() != accepted.generated.len() {
        return Err(Error::new(
            "generated-conflict",
            "Generated plugin file set changed; user data preserved",
        ));
    }
    for (path, digest) in &accepted.generated {
        let file = validation::contained(root, path)?;
        if !file.exists() || storage::digest(&storage::bytes(&file)?) != *digest {
            return Err(Error::new(
                "generated-conflict",
                "Generated plugin differs from accepted bytes; user data preserved",
            ));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for file in &accepted.files {
            let executable = fs::metadata(validation::contained(root, &file.output_path)?)?
                .permissions()
                .mode()
                & 0o111
                != 0;
            if executable != (file.mode == "100755") {
                return Err(Error::new(
                    "mode-conflict",
                    "Generated plugin executable mode changed",
                ));
            }
        }
    }
    Ok(())
}
fn promote(
    root: &Path,
    accepted: &Accepted,
    before: Option<&Accepted>,
    outputs: BTreeMap<String, Vec<u8>>,
    next: &Lock,
) -> Result<()> {
    if let Some(prior) = before {
        check_owned(root, prior)?;
    } else if root
        .join("packages/claude-code")
        .join(&accepted.id)
        .exists()
    {
        return Err(Error::new(
            "generated-conflict",
            "Unowned plugin destination exists",
        ));
    }
    let mut outputs = outputs;
    outputs.insert(LOCK.into(), storage::encoded(next)?);
    for (path, value) in catalog_values(next) {
        outputs.insert(path, storage::encoded(&value)?);
    }
    let mut changes = vec![];
    let mut modes = vec![];
    for (path, bytes) in outputs {
        let physical = validation::contained(root, &path)?;
        let previous = if physical.exists() {
            Some(storage::bytes(&physical)?)
        } else {
            None
        };
        let mode = accepted
            .files
            .iter()
            .find(|f| f.output_path == path)
            .map(|f| if f.mode == "100755" { 0o755 } else { 0o644 });
        let mut before_mode = None;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if physical.exists() {
                before_mode = Some(fs::metadata(&physical)?.permissions().mode() & 0o777);
            }
        }
        let _ = &mut before_mode;
        if previous.as_ref() != Some(&bytes)
            || (mode.is_some() && before_mode.is_some() && mode != before_mode)
        {
            changes.push(Change {
                path: path.clone(),
                before: previous,
                after: Some(bytes),
            });
            if mode.is_some() {
                modes.push((path, before_mode, mode));
            }
        }
    }
    transaction::apply_modes(root, root, changes, &modes)?;
    Ok(())
}
pub fn sync(root: &Path, apply: bool, offline: bool) -> Result<Value> {
    let registry = load(root)?;
    let mut state = lock(root)?;
    check_catalog(root, &state)?;
    let mut statuses = vec![];
    let mut partial = false;
    for collection in &registry.collections {
        crate::jobs::checkpoint()?;
        let repository = match Repository::open(root, collection, offline) {
            Ok(repo) => repo,
            Err(e) => {
                if matches!(e.code, "cancelled" | "timeout") {
                    return Err(e);
                }
                partial = true;
                statuses.push(json!({"collection":collection.id,"status":"failed","code":e.code}));
                continue;
            }
        };
        let revision = match repository.revision(&format!("refs/heads/{}", collection.branch)) {
            Ok(sha) => sha,
            Err(e) => {
                partial = true;
                statuses.push(json!({"collection":collection.id,"status":"failed","code":e.code}));
                continue;
            }
        };
        let tree = repository.tree(&revision)?;
        for skill in &collection.skills {
            crate::jobs::checkpoint()?;
            let key = format!("{}/{}", collection.id, skill.id);
            let prior = state.entries.get(&key).cloned();
            let outcome = (|| -> Result<&'static str> {
                if let Some(prior) = &prior
                    && apply
                    && !root.join("packages/claude-code").join(&prior.id).exists()
                {
                    repository.ensure_revision(&prior.revision, offline)?;
                    let old_selected = selection(&repository.tree(&prior.revision)?, skill)?;
                    let old_payload = repository.blobs(&old_selected)?;
                    policy::verify_license(root, skill, &old_payload)?;
                    let old_files = files(skill, &old_selected, &old_payload)?;
                    if old_files != prior.files {
                        return Err(Error::new(
                            "integrity",
                            "Accepted snapshot differs during reconstruction",
                        ));
                    }
                    let (rebuilt, outputs) = package(
                        root,
                        collection,
                        skill,
                        &prior.revision,
                        prior.version.clone(),
                        old_files,
                        &old_payload,
                    )?;
                    if rebuilt.generated != prior.generated {
                        return Err(Error::new(
                            "integrity",
                            "Accepted plugin differs during reconstruction",
                        ));
                    }
                    promote(root, &rebuilt, Some(prior), outputs, &state)?;
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
                    check_owned(root, prior)?;
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
                            &json!({"sources":skill.notices,"attribution":skill.attribution,"payload":"Exact aggregator payload; upstream adaptations were not made by Forge"}),
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
                let (accepted, outputs) = package(
                    root,
                    collection,
                    skill,
                    accepted_revision,
                    version,
                    candidate_files,
                    &candidate,
                )?;
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
                    partial |= matches!(status, "review-required");
                    statuses.push(json!({"collection":collection.id,"skill":skill.id,"status":status,"revision":revision}));
                }
                Err(e) => {
                    if matches!(e.code, "recovery-required" | "cancelled" | "timeout") {
                        return Err(e);
                    }
                    partial = true;
                    statuses.push(json!({"collection":collection.id,"skill":skill.id,"status":"failed","code":e.code,"message":e.message}));
                }
            }
        }
    }
    if apply {
        catalog(root, &state)?;
    }
    Ok(json!({"status":if partial{"partial"}else{"success"},"concurrency":1,"skills":statuses}))
}
fn catalog_values(state: &Lock) -> [(String, Value); 2] {
    let plugins=state.entries.values().map(|a|json!({"name":a.id,"source":format!("./packages/claude-code/{}",a.id),"version":a.version,"description":format!("Original upstream skill: {}",a.root)})).collect::<Vec<_>>();
    [
        (
            MARKETPLACE.into(),
            json!({"name":"numera-skill-forge","owner":{"name":"KiaroSama"},"plugins":plugins}),
        ),
        (
            "catalogs/collections.json".into(),
            json!({"schema_version":1,"plugins":state.entries.values().map(|a|json!({"id":a.id,"source":a.collection,"root":a.root,"revision":a.revision,"version":a.version,"license":a.license})).collect::<Vec<_>>()}),
        ),
    ]
}
fn check_catalog(root: &Path, state: &Lock) -> Result<()> {
    for (path, value) in catalog_values(state) {
        let physical = validation::contained(root, &path)?;
        if physical.exists() && storage::bytes(&physical)? != storage::encoded(&value)? {
            return Err(Error::new(
                "generated-conflict",
                "Generated marketplace/catalog modified; preserve user edits",
            ));
        }
    }
    Ok(())
}
fn catalog(root: &Path, state: &Lock) -> Result<()> {
    let mut changes = vec![];
    for (path, value) in catalog_values(state) {
        let physical = validation::contained(root, &path)?;
        let before = if physical.exists() {
            Some(storage::bytes(&physical)?)
        } else {
            None
        };
        let after = storage::encoded(&value)?;
        if before.as_ref() != Some(&after) {
            changes.push(Change {
                path,
                before,
                after: Some(after),
            });
        }
    }
    if !changes.is_empty() {
        transaction::apply(root, root, changes)?;
    }
    Ok(())
}
pub fn verify(root: &Path) -> Result<Value> {
    let registry = load(root)?;
    let state = lock(root)?;
    for accepted in state.entries.values() {
        validation::identifier(&accepted.id)?;
        if !root
            .join("packages/claude-code")
            .join(&accepted.id)
            .is_dir()
        {
            return Err(Error::new("integrity", "Accepted plugin is missing"));
        }
        check_owned(root, accepted)?;
        let collection = registry
            .collections
            .iter()
            .find(|c| c.id == accepted.collection)
            .ok_or_else(|| Error::new("collection", "Accepted collection missing registration"))?;
        let skill = collection
            .skills
            .iter()
            .find(|s| s.id == accepted.id)
            .ok_or_else(|| Error::new("collection", "Accepted skill missing registration"))?;
        if skill.root != accepted.root || skill.license != accepted.license {
            return Err(Error::new(
                "approval-stale",
                "Collection registration differs from accepted state",
            ));
        }
        if !skill.license_path.is_empty()
            && !accepted
                .files
                .iter()
                .any(|file| file.path == skill.license_path && file.digest == skill.license_digest)
        {
            return Err(Error::new(
                "approval-stale",
                "Registered license differs from accepted evidence",
            ));
        }
        for (index, notice) in skill.notices.iter().enumerate() {
            let original = storage::bytes(&validation::contained(root, &notice.path)?)?;
            let output = format!(
                "packages/claude-code/{}/upstream-notices/original-{index}.txt",
                skill.id
            );
            if storage::digest(&original) != notice.digest
                || accepted.generated.get(&output) != Some(&notice.digest)
            {
                return Err(Error::new(
                    "approval-stale",
                    "Original notice registration differs from accepted package",
                ));
            }
        }
        let notice_prefix = format!(
            "packages/claude-code/{}/upstream-notices/original-",
            skill.id
        );
        if accepted
            .generated
            .keys()
            .filter(|path| path.starts_with(&notice_prefix))
            .count()
            != skill.notices.len()
        {
            return Err(Error::new(
                "approval-stale",
                "Original notice count differs from acceptance",
            ));
        }
        if !skill.notices.is_empty() {
            if skill.license_path.is_empty() && skill.license_digest != skill.notices[0].digest {
                return Err(Error::new(
                    "approval-stale",
                    "Original license approval differs",
                ));
            }
            let attribution = storage::encoded(
                &json!({"sources":skill.notices,"attribution":skill.attribution,"payload":"Exact aggregator payload; upstream adaptations were not made by Forge"}),
            )?;
            let path = format!(
                "packages/claude-code/{}/upstream-notices/attribution.json",
                skill.id
            );
            if accepted.generated.get(&path) != Some(&storage::digest(&attribution)) {
                return Err(Error::new(
                    "approval-stale",
                    "Original attribution differs from acceptance",
                ));
            }
        }
        for file in &accepted.files {
            if policy::protected(&file.path)
                && skill.reviewed_templates.get(&file.path) != Some(&file.digest)
            {
                return Err(Error::new(
                    "approval-stale",
                    "Protected template approval withdrawn or changed",
                ));
            }
        }
        if accepted.files.iter().any(|f| {
            !accepted.generated.contains_key(&f.output_path)
                || accepted.generated[&f.output_path] != f.digest
        }) {
            return Err(Error::new(
                "integrity",
                "Collection payload differs from ownership record",
            ));
        }
    }
    check_catalog(root, &state)?;
    let catalog: Value = storage::read(&root.join(MARKETPLACE))?;
    if catalog["plugins"].as_array().map(Vec::len) != Some(state.entries.len()) {
        return Err(Error::new(
            "catalog",
            "Marketplace differs from accepted plugin count",
        ));
    }
    Ok(json!({"verified":true,"plugins":state.entries.len(),"execution":"not-performed"}))
}
