use crate::{Error, Result, model::Lock, storage, transaction::Change, validation};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Ownership {
    schema_version: u32,
    target: String,
    files: BTreeMap<String, String>,
}

fn paths(target: &str) -> Vec<String> {
    let mut paths = vec![format!("catalogs/builds/{target}.json")];
    if target == "claude-code" {
        paths.push("packages/claude-code/.claude-plugin/marketplace.json".into());
    }
    paths
}

fn native_entry(root: &Path, id: &str) -> Result<Value> {
    let manifest: Value =
        storage::read(&validation::contained(root, ".claude-plugin/plugin.json")?)?;
    let name = manifest["name"].as_str().filter(|name| {
        !name.is_empty()
            && !name.chars().any(|c| c.is_whitespace() || c.is_control() || "@:/\\".contains(c)
                || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
    }).ok_or_else(|| Error::new("catalog-native", "Native plugin requires a safe nonempty manifest name"))?;
    let mut entry = json!({"name":name,"source":format!("./{id}")});
    for key in ["version", "description"] {
        if let Some(value) = manifest.get(key) {
            if !value.is_string() {
                return Err(Error::new(
                    "catalog-native",
                    "Native version and description must be strings when present",
                ));
            }
            entry[key] = value.clone();
        }
    }
    Ok(entry)
}

fn values(
    workspace: &Path,
    target: &str,
    lock: &Lock,
    staged: Option<(&str, &Path)>,
) -> Result<BTreeMap<String, Vec<u8>>> {
    if lock.builds.len() > storage::MAX_FILES {
        return Err(Error::new(
            "size-limit",
            "Build catalog exceeds the package limit",
        ));
    }
    let mut packages = Vec::new();
    let mut plugins = Vec::new();
    let mut names = BTreeMap::new();
    for (key, identity) in &lock.builds {
        let Some((profile, id)) = key.split_once('/') else {
            return Err(Error::new(
                "catalog-integrity",
                "Build identity requires target/package",
            ));
        };
        if profile != target {
            continue;
        }
        validation::identifier(id)?;
        let physical = validation::contained(workspace, &format!("packages/{target}/{id}"))?;
        let root = staged
            .filter(|(selected, _)| *selected == id)
            .map_or(physical.as_path(), |(_, stage)| stage);
        if !root.is_dir() || crate::build::tree_digest(root)? != identity.payload_digest {
            return Err(Error::new(
                "generated-conflict",
                "Catalog package differs from its owned build identity",
            ));
        }
        let mut package = json!({"id":id,"path":format!("packages/{target}/{id}"),"version":identity.version,"digest":identity.payload_digest});
        if target == "claude-code" {
            let entry = native_entry(root, id)?;
            let name = entry["name"]
                .as_str()
                .ok_or_else(|| Error::new("catalog-native", "Native plugin name missing"))?;
            if names.insert(name.to_lowercase(), id.to_owned()).is_some() {
                return Err(Error::new(
                    "catalog-collision",
                    "Built plugins have duplicate or case-colliding native names",
                ));
            }
            package["native_name"] = entry["name"].clone();
            if let Some(version) = entry.get("version") {
                package["native_version"] = version.clone();
            }
            plugins.push(entry);
        }
        packages.push(package);
    }
    let mut outputs = BTreeMap::new();
    outputs.insert(
        format!("catalogs/builds/{target}.json"),
        storage::encoded(&json!({"schema_version":1,"target":target,"packages":packages}))?,
    );
    if target == "claude-code" {
        outputs.insert("packages/claude-code/.claude-plugin/marketplace.json".into(), storage::encoded(&json!({"name":"numera-forge-builds","owner":{"name":"Numera Skill Forge"},"plugins":plugins}))?);
    }
    Ok(outputs)
}

/// Plan workspace metadata only; the build transaction promotes it together with the payload.
pub(crate) fn prepare(
    workspace: &Path,
    target: &str,
    id: &str,
    stage: &Path,
    next: &Lock,
) -> Result<(Vec<Change>, Vec<String>)> {
    let expected_paths = paths(target);
    let ownership_path = format!("catalogs/builds/{target}.lock.json");
    let owner_path = validation::contained(workspace, &ownership_path)?;
    let owner_before = if owner_path.exists() {
        Some(storage::bytes(&owner_path)?)
    } else {
        None
    };
    if let Some(bytes) = &owner_before {
        let owner: Ownership = serde_json::from_slice(bytes)?;
        if owner.schema_version != 1
            || owner.target != target
            || owner.files.len() != expected_paths.len()
            || expected_paths
                .iter()
                .any(|path| !owner.files.contains_key(path))
        {
            return Err(Error::new(
                "catalog-integrity",
                "Catalog ownership metadata differs from the selected target",
            ));
        }
        for (path, digest) in owner.files {
            let physical = validation::contained(workspace, &path)?;
            if !physical.is_file() || storage::digest(&storage::bytes(&physical)?) != digest {
                return Err(Error::new(
                    "generated-conflict",
                    "Owned catalog was changed or removed; user changes preserved",
                ));
            }
        }
    } else {
        if expected_paths
            .iter()
            .any(|path| workspace.join(path).exists())
        {
            return Err(Error::new(
                "generated-conflict",
                "Existing catalog has no ownership record; user changes preserved",
            ));
        }
    }
    let outputs = values(workspace, target, next, Some((id, stage)))?;
    let ownership = Ownership {
        schema_version: 1,
        target: target.into(),
        files: outputs
            .iter()
            .map(|(path, bytes)| (path.clone(), storage::digest(bytes)))
            .collect(),
    };
    let mut changes = Vec::new();
    for (path, after) in outputs {
        let physical = validation::contained(workspace, &path)?;
        let before = if physical.exists() {
            Some(storage::bytes(&physical)?)
        } else {
            None
        };
        changes.push(Change {
            path,
            before,
            after: Some(after),
        });
    }
    changes.push(Change {
        path: ownership_path,
        before: owner_before,
        after: Some(storage::encoded(&ownership)?),
    });
    Ok((changes, expected_paths))
}
