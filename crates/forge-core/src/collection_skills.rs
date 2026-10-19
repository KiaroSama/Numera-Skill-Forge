use crate::{Error, Result, adapters, collections::model::Accepted, storage, validation};
use serde_json::{Value, json};
use std::path::Path;

pub(crate) fn entries(root: &Path) -> Result<Vec<Accepted>> {
    let path = root.join("catalogs/collections.lock.json");
    if !path.exists() {
        return Ok(vec![]);
    }
    let lock: crate::collections::model::Lock = storage::read(&path)?;
    if lock.schema_version != 1 {
        return Err(Error::new("schema-version", "Unsupported collection lock"));
    }
    let mut ids = std::collections::BTreeSet::new();
    for entry in lock.entries.values() {
        validation::identifier(&entry.id)?;
        if !ids.insert(&entry.id) {
            return Err(Error::new(
                "duplicate-id",
                "Accepted package identities collide",
            ));
        }
    }
    Ok(lock.entries.into_values().collect())
}

pub(crate) fn find(root: &Path, id: &str) -> Result<Option<Accepted>> {
    let accepted = entries(root)?.into_iter().find(|a| a.id == id);
    if accepted.is_some()
        && crate::registry::load(root)?
            .packages
            .iter()
            .any(|p| p.id == id)
    {
        return Err(Error::new(
            "package-identity-conflict",
            "Generic and collection packages share an ID; resolve the registration before use",
        ));
    }
    Ok(accepted)
}

pub(crate) fn metadata(root: &Path, accepted: &Accepted) -> Result<(String, String)> {
    let prefix = format!("packages/claude-code/{}/skills/", accepted.id);
    let generated = crate::collections::guide::entry_path(accepted)
        .or_else(|| crate::collections::design::entry_path(accepted));
    let path = generated
        .or_else(|| {
            accepted
                .files
                .iter()
                .find(|f| {
                    f.output_path.starts_with(&prefix)
                        && f.output_path.ends_with("/SKILL.md")
                        && f.output_path[prefix.len()..].split('/').count() == 2
                })
                .map(|f| f.output_path.clone())
        })
        .ok_or_else(|| Error::new("skill-entrypoint", "Accepted skill entry point is missing"))?;
    let digest = accepted
        .generated
        .get(&path)
        .ok_or_else(|| Error::new("integrity", "Skill entry is not owned"))?;
    let bytes = storage::bytes(&validation::contained(root, &path)?)?;
    if &storage::digest(&bytes) != digest {
        return Err(Error::new("integrity", "Accepted skill bytes changed"));
    }
    let text =
        std::str::from_utf8(&bytes).map_err(|_| Error::new("encoding", "Skill is not UTF-8"))?;
    let front = validation::frontmatter(text)
        .ok_or_else(|| Error::new("frontmatter", "Original skill frontmatter is missing"))?;
    let value: Value = serde_yaml_ng::from_str(front)
        .map_err(|_| Error::new("frontmatter", "Original skill frontmatter is invalid"))?;
    let name = value["name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::new("frontmatter", "Original skill requires a name"))?;
    validation::identifier(name)?;
    if path != format!("{prefix}{name}/SKILL.md") {
        return Err(Error::new(
            "integrity",
            "Original skill directory does not match its name",
        ));
    }
    let description = value["description"]
        .as_str()
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Ok((name.into(), description))
}

pub(crate) fn assess(root: &Path, accepted: &Accepted, target: &str) -> Result<Value> {
    adapters::install_prefix(target)?;
    let (name, description) = metadata(root, accepted)?;
    assess_metadata(accepted, target, &name, &description)
}

pub(crate) fn assess_metadata(
    accepted: &Accepted,
    target: &str,
    name: &str,
    description: &str,
) -> Result<Value> {
    let strict = matches!(
        target,
        "opencode" | "opencode-v1" | "copilot" | "vscode" | "cline" | "kilo" | "kiro"
    );
    let invalid_name = name.chars().count() > 64
        || (strict
            && (!name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                || name.starts_with('-')
                || name.ends_with('-')
                || name.contains("--")));
    let limited = matches!(
        target,
        "codex" | "opencode" | "opencode-v1" | "copilot" | "vscode" | "cline" | "kilo" | "kiro"
    );
    if target != "opencode-v2" && description.is_empty() {
        return Err(Error::new(
            "skill-metadata",
            "Original skill description is required by this host",
        ));
    }
    if limited && (invalid_name || description.is_empty() || description.chars().count() > 1024) {
        return Err(Error::new(
            "skill-metadata",
            format!(
                "Original skill {name} is incompatible with {target} metadata limits; original bytes preserved"
            ),
        ));
    }
    if target == "codex" && accepted.id.chars().count() + name.chars().count() + 1 > 129 {
        return Err(Error::new(
            "skill-metadata",
            "Qualified Codex skill identity exceeds 129 characters",
        ));
    }
    Ok(
        json!({"supported":true,"target":target,"package":accepted.id,"skill":name,
        "delivery":"standalone original skills; host enablement/trust separate",
        "verification":"metadata/path contract; not original-host runtime execution"}),
    )
}

pub(crate) fn catalog(root: &Path) -> Result<Vec<Value>> {
    entries(root)?
        .iter()
        .map(|a| {
            let mut row = json!({"id":a.id,"source":a.collection,"version":a.version,
                "collection":true,"revision":a.revision,"license":a.license});
            match metadata(root, a) {
                Ok((name, _)) => row["name"] = json!(name),
                Err(error) => {
                    row["name"] = json!(a.id);
                    row["error"] = json!({"code":error.code,"message":error.message});
                }
            }
            Ok(row)
        })
        .collect()
}
