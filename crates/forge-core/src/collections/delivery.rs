use super::*;

pub(crate) const CURSOR_CATALOG: &str = ".cursor-plugin/marketplace.json";

pub(super) fn descriptors(a: &Accepted) -> Result<BTreeMap<String, Vec<u8>>> {
    let prefix = format!("packages/claude-code/{}", a.id);
    let description = if a.files.iter().any(|f| f.source_digest.is_some()) {
        format!(
            "Upstream skill with reviewed name-only adaptation: {}",
            a.root
        )
    } else {
        format!("Unmodified upstream skill: {}", a.root)
    };
    let native = json!({"name":a.id,"version":a.version,
        "description":description,
        "license":a.license,"skills":"./skills/"});
    let portable = json!({"$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
        "name":a.id,"version":a.version,"description":description,
        "license":a.license});
    [
        (
            format!("{prefix}/.codex-plugin/plugin.json"),
            native.clone(),
        ),
        (format!("{prefix}/.cursor-plugin/plugin.json"), native),
        (format!("{prefix}/plugin.json"), portable),
    ]
    .into_iter()
    .map(|(path, value)| Ok((path, storage::encoded(&value)?)))
    .collect()
}

pub(super) fn catalogs(state: &Lock) -> [(String, Value); 1] {
    let cursor = state
        .entries
        .values()
        .map(|a| {
            json!({"name":a.id,
        "source":format!("packages/claude-code/{}",a.id),
        "description":format!("Original upstream skill: {}",a.root)})
        })
        .collect::<Vec<_>>();
    [(
        CURSOR_CATALOG.into(),
        json!({"name":"numera-skill-forge","owner":{"name":"KiaroSama"},"plugins":cursor}),
    )]
}

/// Add only deterministic host metadata; accepted original identity and bytes never change.
pub(super) fn prepare(root: &Path, state: &mut Lock, id: &str) -> Result<()> {
    super::verify_selected(root, Some(id))?;
    check_catalog(root, state)?;
    let mut next = Lock {
        schema_version: 1,
        entries: state.entries.clone(),
    };
    let mut changes = vec![];
    for accepted in next.entries.values_mut().filter(|a| a.id == id) {
        let dir = root.join("packages/claude-code").join(&accepted.id);
        if !dir.exists() {
            continue;
        }
        check_owned(root, accepted)?;
        let expected = descriptors(accepted)?;
        let hashes = expected
            .iter()
            .map(|(p, b)| (p.clone(), storage::digest(b)))
            .collect::<BTreeMap<_, _>>();
        if !accepted.delivery.is_empty() && accepted.delivery != hashes {
            return Err(Error::new(
                "generated-conflict",
                "Native delivery ownership differs from its contract",
            ));
        }
        for (path, bytes) in expected {
            let physical = validation::contained(root, &path)?;
            let before = if physical.exists() {
                Some(storage::bytes(&physical)?)
            } else {
                None
            };
            if before.as_ref().is_some_and(|b| b != &bytes) {
                return Err(Error::new(
                    "generated-conflict",
                    "Native descriptor modified; preserve user data",
                ));
            }
            if before.is_none() {
                changes.push(Change {
                    path,
                    before,
                    after: Some(bytes),
                });
            }
        }
        accepted.delivery = hashes;
    }
    let before = if root.join(LOCK).exists() {
        Some(storage::bytes(&root.join(LOCK))?)
    } else {
        None
    };
    let after = storage::encoded(&next)?;
    if before.as_ref() != Some(&after) {
        changes.push(Change {
            path: LOCK.into(),
            before,
            after: Some(after),
        });
    }
    for (path, value) in catalog_values(&next) {
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
    *state = next;
    Ok(())
}

pub fn build(root: &Path, id: &str, target: &str) -> Result<Value> {
    let mut state = lock(root)?;
    let selected = state
        .entries
        .values()
        .find(|a| a.id == id)
        .ok_or_else(|| Error::new("package", "Unknown accepted collection package"))?;
    crate::collection_skills::assess(root, selected, target)?;
    if selected.delivery.is_empty() {
        prepare(root, &mut state, id)?;
    }
    let accepted = state
        .entries
        .values()
        .find(|a| a.id == id)
        .ok_or_else(|| Error::new("package", "Unknown accepted collection package"))?;
    super::verify_selected(root, Some(id))?;
    catalog(root, &state)?;
    crate::collection_skills::assess(root, accepted, target)?;
    Ok(
        json!({"package":id,"target":target,"version":accepted.version,
        "digest":accepted.content_digest,"path":root.join("packages/claude-code").join(id),
        "delivery":"shared original skill payload; native descriptors available",
        "execution":"not-performed"}),
    )
}
