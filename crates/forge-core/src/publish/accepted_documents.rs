use super::Git;
use crate::{Error, Result, storage, validation};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn instructional(path: &str) -> bool {
    path.rsplit('/').next().is_some_and(|name| {
        matches!(
            name.to_ascii_lowercase().as_str(),
            "claude.md" | "agents.md" | "reference.md"
        )
    })
}

fn blobs(git: &Git<'_>, paths: &[String]) -> Result<Vec<Option<Vec<u8>>>> {
    let input = paths
        .iter()
        .map(|p| format!("HEAD:{p}\n"))
        .collect::<String>();
    let output = git.run(&["cat-file", "--batch"], Some(input.into_bytes()))?;
    let mut remaining = output.as_slice();
    let mut result = Vec::new();
    for _ in paths {
        let end = remaining.iter().position(|b| *b == b'\n').ok_or_else(|| {
            Error::new("publication-baseline", "Incomplete committed blob response")
        })?;
        let header = std::str::from_utf8(&remaining[..end])
            .map_err(|_| Error::new("encoding", "Invalid committed blob header"))?;
        remaining = &remaining[end + 1..];
        if header.ends_with(" missing") {
            result.push(None);
            continue;
        }
        let fields: Vec<_> = header.split_whitespace().collect();
        let size = fields
            .get(2)
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| *n <= storage::MAX_TOTAL as usize);
        let Some(size) = size else {
            return Err(Error::new(
                "publication-baseline",
                "Invalid committed blob size",
            ));
        };
        if fields.len() != 3
            || fields[1] != "blob"
            || remaining.len() <= size
            || remaining[size] != b'\n'
        {
            return Err(Error::new(
                "publication-baseline",
                "Invalid committed blob response",
            ));
        }
        result.push(Some(remaining[..size].to_vec()));
        remaining = &remaining[size + 1..];
    }
    if !remaining.is_empty() {
        return Err(Error::new(
            "publication-baseline",
            "Unexpected committed blob response",
        ));
    }
    Ok(result)
}

/// Only committed accepted bytes classify public instructions; a working lock cannot self-approve.
pub(super) fn baseline(git: &Git<'_>) -> Result<BTreeMap<String, String>> {
    let mut documents = BTreeMap::new();
    let metadata = blobs(
        git,
        &[
            "catalogs/collections.lock.json".into(),
            "registry/collections.json".into(),
        ],
    )?;
    let (Some(lock), Some(registry)) = (&metadata[0], &metadata[1]) else {
        return Ok(documents);
    };
    let lock: Value = serde_json::from_slice(lock)?;
    let registry: Value = serde_json::from_slice(registry)?;
    if lock["schema_version"] != 1 || registry["schema_version"] != 1 {
        return Ok(documents);
    }
    let Some(entries) = lock["entries"].as_object() else {
        return Ok(documents);
    };
    let mut candidates = Vec::new();
    for (key, entry) in entries {
        let (Some(id), Some(collection), Some(root)) = (
            entry["id"].as_str(),
            entry["collection"].as_str(),
            entry["root"].as_str(),
        ) else {
            continue;
        };
        if key != &format!("{collection}/{id}")
            || validation::identifier(id).is_err()
            || validation::identifier(collection).is_err()
            || validation::relative_path(root).is_err()
        {
            continue;
        }
        let registered = registry["collections"]
            .as_array()
            .is_some_and(|collections| {
                collections.iter().any(|origin| {
                    origin["id"] == collection
                        && origin["skills"].as_array().is_some_and(|skills| {
                            skills
                                .iter()
                                .any(|skill| skill["id"] == id && skill["root"] == root)
                        })
                })
            });
        if !registered {
            continue;
        }
        let Some(files) = entry["files"].as_array() else {
            continue;
        };
        let Some(skill) = files
            .iter()
            .find(|f| f["path"] == format!("{root}/SKILL.md"))
        else {
            continue;
        };
        let (Some(skill_output), Some(skill_digest)) =
            (skill["output_path"].as_str(), skill["digest"].as_str())
        else {
            continue;
        };
        if validation::relative_path(skill_output).is_err()
            || entry["generated"][skill_output] != skill_digest
        {
            continue;
        }
        for file in files {
            let (Some(source), Some(output), Some(digest)) = (
                file["path"].as_str(),
                file["output_path"].as_str(),
                file["digest"].as_str(),
            ) else {
                continue;
            };
            let Some(relative) = source.strip_prefix(&format!("{root}/")) else {
                continue;
            };
            if !instructional(output)
                || validation::relative_path(output).is_err()
                || validation::relative_path(relative).is_err()
                || entry["generated"][output] != digest
            {
                continue;
            }
            candidates.push((id, relative, output, digest, skill_output, skill_digest));
        }
    }
    let paths = candidates
        .iter()
        .flat_map(|c| [c.2.to_owned(), c.4.to_owned()])
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Ok(documents);
    }
    let contents = blobs(git, &paths)?;
    for (candidate, bytes) in candidates.iter().zip(contents.as_chunks::<2>().0) {
        let (Some(document), Some(skill)) = (&bytes[0], &bytes[1]) else {
            continue;
        };
        let (id, relative, output, digest, skill_output, skill_digest) = *candidate;
        if storage::digest(document) != digest || storage::digest(skill) != skill_digest {
            continue;
        }
        let text = std::str::from_utf8(skill)
            .map_err(|_| Error::new("encoding", "Committed skill is not UTF-8"))?;
        let Some(front) = text
            .strip_prefix("---\n")
            .and_then(|s| s.split_once("\n---"))
            .map(|v| v.0)
        else {
            continue;
        };
        let metadata: Value = serde_yaml_ng::from_str(front)
            .map_err(|_| Error::new("publication-baseline", "Invalid committed skill metadata"))?;
        let Some(name) = metadata["name"].as_str() else {
            continue;
        };
        if validation::identifier(name).is_err() {
            continue;
        }
        let prefix = format!("packages/claude-code/{id}/skills/{name}/");
        if skill_output == format!("{prefix}SKILL.md") && output == format!("{prefix}{relative}") {
            documents.insert(output.into(), digest.into());
        }
    }
    Ok(documents)
}
