use super::{
    acquire::Blob,
    model::{Accepted, Skill},
    policy,
};
use crate::{Error, Result, storage, validation};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

fn valid_source(source: &str) -> bool {
    let Some(rest) = source.strip_prefix("https://github.com/") else {
        return false;
    };
    let parts = rest.split('/').collect::<Vec<_>>();
    parts.len() >= 5
        && !parts[0].is_empty()
        && !parts[1].is_empty()
        && parts[2] == "blob"
        && parts[3].len() == 40
        && parts[3].bytes().all(|b| b.is_ascii_hexdigit())
        && !source.contains(['@', '?', '#', '\r', '\n'])
}

fn valid_supplement(s: &super::model::ReviewedSupplement) -> bool {
    if let Some(artifact) = &s.artifact {
        let Some(path) = s.source.strip_prefix("https://registry.npmjs.org/") else {
            return false;
        };
        let parts = path.split('/').collect::<Vec<_>>();
        parts.len() == 3
            && parts[1] == "-"
            && parts[2].ends_with(".tgz")
            && parts[0]
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !s.source.contains(['@', '?', '#', '\r', '\n'])
            && artifact.digest.len() == 64
            && artifact.digest.bytes().all(|b| b.is_ascii_hexdigit())
            && artifact.integrity.starts_with("sha512-")
            && artifact.integrity.len() == 95
            && artifact.member.starts_with("package/")
            && validation::relative_path(&artifact.member).is_ok()
    } else {
        valid_source(&s.source)
    }
}

fn invalid() -> Error {
    Error::new(
        "distribution-review",
        "Reviewed distribution source, layout or version differs",
    )
}

fn native_output(skill: &Skill, source: &str, output: &str) -> bool {
    skill
        .reviewed_distribution
        .as_ref()
        .and_then(|r| r.native.as_ref())
        .is_some_and(|n| {
            (n.files.contains_key(source) || n.hook_configs.contains(&source.to_owned()))
                && output.starts_with("native-originals/")
        })
}

pub fn validate(skill: &Skill) -> Result<()> {
    let Some(review) = &skill.reviewed_distribution else {
        return Ok(());
    };
    if review.version != 1
        || review.files.is_empty()
        || review.files.len() > storage::MAX_FILES
        || skill.reviewed_name.is_some()
        || skill.reviewed_design.is_some()
        || skill.reviewed_guide.is_some()
    {
        return Err(invalid());
    }
    let mut outputs = BTreeSet::from(["upstream-notices/distribution.json".to_owned()]);
    if !skill.notices.is_empty() {
        outputs.insert("upstream-notices/attribution.json".into());
        for index in 0..skill.notices.len() {
            outputs.insert(format!("upstream-notices/original-{index}.txt"));
        }
    }
    for (path, file) in &review.files {
        validation::relative_path(path)?;
        validation::relative_path(&file.output)?;
        if policy::protected(path)
            && !skill.reviewed_templates.contains_key(path)
            && !skill.reviewed_documents.contains_key(path)
            || policy::protected(&file.output)
                && !(skill.reviewed_templates.contains_key(path)
                    && file.output.ends_with("/.env.example")
                    || skill.reviewed_documents.contains_key(path)
                        && file.output.ends_with("/references/secrets.md"))
            || !matches!(file.mode.as_str(), "100644" | "100755")
            || file.digest.len() != 64
            || !file
                .digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !(file.output.starts_with("skills/")
                || file.output.starts_with("upstream-notices/")
                || native_output(skill, path, &file.output))
            || file.output == "upstream-notices/distribution.json"
            || file.change_notice.as_ref().is_some_and(|notice| {
                !path.ends_with(".md")
                    || !notice.starts_with("Forge distribution notice:")
                    || notice.contains(['\0', '\r'])
                    || notice.len() > 4096
            })
            || !outputs.insert(file.output.to_ascii_lowercase())
        {
            return Err(invalid());
        }
    }
    for (path, supplement) in &review.supplements {
        validation::relative_path(path)?;
        validation::relative_path(&supplement.output)?;
        if !path.starts_with("registry/licenses/")
            || policy::protected(path)
            || policy::protected(&supplement.output)
            || supplement.digest.len() != 64
            || !supplement
                .digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !(supplement.output.starts_with("skills/")
                || supplement.output.starts_with("upstream-notices/"))
            || supplement.output == "upstream-notices/distribution.json"
            || !valid_supplement(supplement)
            || supplement.license.is_empty()
            || !outputs.insert(supplement.output.to_ascii_lowercase())
        {
            return Err(invalid());
        }
    }
    for (path, file) in &review.files {
        if file.change_notice.is_some()
            && !outputs
                .insert(format!("upstream-notices/original-files/{path}").to_ascii_lowercase())
        {
            return Err(invalid());
        }
    }
    let mut scoped = BTreeSet::new();
    for obligation in &review.obligations {
        if obligation.files.is_empty()
            || obligation.notices.is_empty()
            || obligation.attribution.trim().is_empty()
            || !valid_source(&obligation.source)
            || !matches!(
                obligation.license.as_str(),
                "MIT" | "Apache-2.0" | "CC-BY-4.0" | "GPL-3.0-only" | "CC-BY-SA-4.0"
            )
        {
            return Err(invalid());
        }
        for path in &obligation.files {
            if !review.files.contains_key(path) || !scoped.insert(path) {
                return Err(invalid());
            }
        }
        for notice in &obligation.notices {
            if !review.files.contains_key(notice) && !review.supplements.contains_key(notice) {
                return Err(invalid());
            }
        }
    }
    if !review.obligations.is_empty() && scoped.len() != review.files.len() {
        return Err(invalid());
    }
    let declared = skill.license.split(" AND ").collect::<BTreeSet<_>>();
    if declared.iter().any(|term| {
        !matches!(
            *term,
            "MIT" | "Apache-2.0" | "CC-BY-4.0" | "GPL-3.0-only" | "CC-BY-SA-4.0"
        )
    }) || (declared.len() > 1
        || declared.contains("GPL-3.0-only")
        || declared.contains("CC-BY-SA-4.0"))
        && (review.obligations.is_empty()
            || review
                .obligations
                .iter()
                .map(|o| o.license.as_str())
                .collect::<BTreeSet<_>>()
                != declared)
    {
        return Err(invalid());
    }
    let paths = outputs.iter().collect::<Vec<_>>();
    if paths.iter().any(|path| {
        paths
            .iter()
            .any(|other| path != other && other.starts_with(&format!("{path}/")))
    }) {
        return Err(invalid());
    }
    Ok(())
}

pub fn terms_match(license: &str, bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    match license {
        "MIT" => text.contains("Permission is hereby granted"),
        "Apache-2.0" => text.contains("Apache License") && text.contains("Version 2.0"),
        "GPL-3.0-only" => {
            text.contains("GNU GENERAL PUBLIC LICENSE") && text.contains("Version 3, 29 June 2007")
        }
        "CC-BY-SA-4.0" => {
            text.contains("Attribution-ShareAlike 4.0")
                || text.contains("Creative Commons Attribution-ShareAlike 4.0")
        }
        "CC-BY-4.0" => text.contains("Attribution 4.0"),
        _ => false,
    }
}

pub fn obligations(root: &Path, skill: &Skill, payload: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    let Some(review) = &skill.reviewed_distribution else {
        return Ok(());
    };
    for obligation in &review.obligations {
        let mut matching_terms = false;
        for path in &obligation.notices {
            let bytes = if let Some(bytes) = payload.get(path) {
                bytes.clone()
            } else {
                let supplement = review.supplements.get(path).ok_or_else(invalid)?;
                let bytes = storage::bytes(&validation::contained(root, path)?)?;
                if storage::digest(&bytes) != supplement.digest {
                    return Err(invalid());
                }
                bytes
            };
            matching_terms |= terms_match(&obligation.license, &bytes);
        }
        if !matching_terms {
            return Err(invalid());
        }
    }
    Ok(())
}

pub fn source(skill: &Skill, selected: &[Blob], payload: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    let Some(review) = &skill.reviewed_distribution else {
        return Ok(());
    };
    validate(skill)?;
    if review.files.len() != selected.len() {
        return Err(invalid());
    }
    for blob in selected {
        let expected = review.files.get(&blob.path).ok_or_else(invalid)?;
        let bytes = payload.get(&blob.path).ok_or_else(invalid)?;
        if expected.mode != blob.mode || expected.digest != storage::digest(bytes) {
            return Err(invalid());
        }
    }
    Ok(())
}

pub fn payload(skill: &Skill, source: &str, original: &[u8]) -> Result<Vec<u8>> {
    let notice = skill
        .reviewed_distribution
        .as_ref()
        .and_then(|r| r.files.get(source))
        .and_then(|f| f.change_notice.as_ref());
    let mut emitted = original.to_vec();
    if let Some(notice) = notice {
        emitted.extend_from_slice(format!("\n\n---\n{notice}\n").as_bytes());
    }
    Ok(emitted)
}

pub fn output(skill: &Skill, source: &str, name: &str) -> Result<Option<String>> {
    let Some(review) = &skill.reviewed_distribution else {
        return Ok(None);
    };
    let file = review.files.get(source).ok_or_else(invalid)?;
    let expected_entry = format!("skills/{name}/SKILL.md");
    if source == format!("{}/SKILL.md", skill.root) && file.output != expected_entry
        || file.output.starts_with("skills/")
            && !file.output.starts_with(&format!("skills/{name}/"))
    {
        return Err(invalid());
    }
    Ok(Some(format!(
        "packages/claude-code/{}/{}",
        skill.id, file.output
    )))
}

pub fn outputs(root: &Path, skill: &Skill) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut result = BTreeMap::new();
    if let Some(review) = &skill.reviewed_distribution {
        validate(skill)?;
        for (path, supplement) in &review.supplements {
            let bytes = storage::bytes(&validation::contained(root, path)?)?;
            if storage::digest(&bytes) != supplement.digest {
                return Err(invalid());
            }
            result.insert(
                format!("packages/claude-code/{}/{}", skill.id, supplement.output),
                bytes,
            );
        }
        result.insert(
            format!(
                "packages/claude-code/{}/upstream-notices/distribution.json",
                skill.id
            ),
            storage::encoded(review)?,
        );
    }
    Ok(result)
}

pub fn verify(root: &Path, skill: &Skill, accepted: &Accepted) -> Result<()> {
    let stale = || {
        Error::new(
            "approval-stale",
            "Reviewed distribution approval differs from accepted source and layout",
        )
    };
    if skill.reviewed_distribution != accepted.reviewed_distribution {
        return Err(stale());
    }
    let Some(review) = &skill.reviewed_distribution else {
        return Ok(());
    };
    validate(skill).map_err(|_| stale())?;
    if review.files.len() != accepted.files.len() {
        return Err(stale());
    }
    for file in &accepted.files {
        let expected = review.files.get(&file.path).ok_or_else(stale)?;
        let original = if expected.change_notice.is_some() {
            storage::bytes(&validation::contained(
                root,
                &format!(
                    "packages/claude-code/{}/upstream-notices/original-files/{}",
                    skill.id, file.path
                ),
            )?)?
        } else {
            storage::bytes(&validation::contained(root, &file.output_path)?)?
        };
        let emitted = payload(skill, &file.path, &original)?;
        if expected.digest != storage::digest(&original)
            || file.digest != storage::digest(&emitted)
            || file.size != emitted.len() as u64
            || expected.mode != file.mode
            || file.source_digest
                != expected
                    .change_notice
                    .as_ref()
                    .map(|_| expected.digest.clone())
            || file.output_path != format!("packages/claude-code/{}/{}", skill.id, expected.output)
        {
            return Err(stale());
        }
    }
    let mut payload = BTreeMap::new();
    for file in &accepted.files {
        payload.insert(
            file.path.clone(),
            storage::bytes(&validation::contained(root, &file.output_path)?)?,
        );
    }
    obligations(root, skill, &payload).map_err(|_| stale())?;
    for (path, bytes) in outputs(root, skill)? {
        if accepted.generated.get(&path) != Some(&storage::digest(&bytes))
            || storage::bytes(&validation::contained(root, &path)?)? != bytes
        {
            return Err(stale());
        }
    }
    Ok(())
}
