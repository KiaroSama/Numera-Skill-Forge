use super::model::{Accepted, File, Skill};
use crate::{Error, Result, storage, validation};
use std::collections::BTreeMap;

pub fn validate_skill(skill: &Skill) -> Result<()> {
    validation::identifier(&skill.id)?;
    validation::relative_path(&skill.root)?;
    if let Some(approval) = &skill.reviewed_name {
        super::adaptation::validate(approval)?;
    }
    if !skill.license_path.is_empty() {
        validation::relative_path(&skill.license_path)?;
    }
    if skill.license_path.is_empty() && skill.notices.is_empty() {
        return Err(Error::new(
            "license-review",
            "Supply original license evidence",
        ));
    }
    for (path, digest) in &skill.reviewed_templates {
        validation::relative_path(path)?;
        if !path.starts_with(&format!("{}/", skill.root))
            || !path.ends_with("/.env.example")
            || path
                .rsplit_once('/')
                .is_none_or(|(parent, _)| protected(parent))
            || digest.len() != 64
            || !digest.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Error::new(
                "protected-payload",
                "Only exact reviewed public environment templates may be admitted",
            ));
        }
    }
    for (path, digest) in &skill.reviewed_documents {
        validation::relative_path(path)?;
        let canonical = (skill.root.starts_with("skills/") && skill.root.split('/').count() == 2)
            || (skill.root.starts_with("cli-tool/components/skills/")
                && skill.root.split('/').count() == 5);
        if !canonical
            || path != &format!("{}/references/secrets.md", skill.root)
            || protected(&skill.root)
            || digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::new(
                "protected-payload",
                "Only exact reviewed public reference documents may be admitted",
            ));
        }
    }
    for notice in &skill.notices {
        validation::relative_path(&notice.path)?;
        if !notice.path.starts_with("registry/licenses/")
            || notice.digest.len() != 64
            || !notice.digest.bytes().all(|b| b.is_ascii_hexdigit())
            || !notice.source.starts_with("https://github.com/")
            || notice.source.contains(['@', '?', '#', '\r', '\n'])
        {
            return Err(Error::new(
                "license-review",
                "Invalid original notice identity",
            ));
        }
    }
    if !matches!(skill.license.as_str(), "MIT" | "Apache-2.0" | "CC-BY-4.0")
        || skill.license_digest.len() != 64
        || !skill.license_digest.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(Error::new(
            "license-review",
            "Collection requires explicitly reviewed permissive license evidence",
        ));
    }
    Ok(())
}
pub fn protected(path: &str) -> bool {
    path.split('/').any(|part| {
        matches!(
            part.to_ascii_lowercase().as_str(),
            ".git" | ".ai" | ".ignoreme" | "secrets.md" | "explain-ai.md" | "node_modules"
        ) || part.starts_with(".env")
    })
}
pub fn license(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or("").to_ascii_uppercase();
    name.starts_with("LICENSE") || name.starts_with("COPYING") || name.starts_with("NOTICE")
}
pub fn verify_license(
    root: &std::path::Path,
    skill: &Skill,
    payload: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    for (path, digest) in &skill.reviewed_templates {
        let bytes = payload.get(path).ok_or_else(|| {
            Error::new("protected-payload", "Reviewed environment template missing")
        })?;
        if storage::digest(bytes) != *digest {
            return Err(Error::new(
                "protected-payload",
                "Environment template changed; review placeholders before publication",
            ));
        }
    }
    for (path, digest) in &skill.reviewed_documents {
        if payload
            .get(path)
            .is_none_or(|bytes| storage::digest(bytes) != *digest)
        {
            return Err(Error::new(
                "protected-payload",
                "Reviewed public document missing or changed",
            ));
        }
    }
    let external;
    let bytes = if skill.license_path.is_empty() {
        let notice = skill
            .notices
            .first()
            .ok_or_else(|| Error::new("license-review", "Missing original grant"))?;
        external = storage::bytes(&validation::contained(root, &notice.path)?)?;
        &external
    } else {
        payload
            .get(&skill.license_path)
            .ok_or_else(|| Error::new("license-review", "Missing reviewed license file"))?
    };
    for notice in &skill.notices {
        let bytes = storage::bytes(&validation::contained(root, &notice.path)?)?;
        if storage::digest(&bytes) != notice.digest {
            return Err(Error::new(
                "license-review",
                "Original notice differs from reviewed identity",
            ));
        }
    }
    if storage::digest(bytes) != skill.license_digest {
        return Err(Error::new(
            "license-review",
            "License differs from reviewed identity",
        ));
    }
    let text = String::from_utf8_lossy(bytes);
    let expected = if skill.license == "MIT" {
        text.contains("Permission is hereby granted")
    } else if skill.license == "Apache-2.0" {
        (text.contains("Apache License") && text.contains("Version 2.0"))
            || (skill.license_path.is_empty() && text.contains("Apache") && text.contains("2.0"))
    } else {
        text.contains("Attribution 4.0") || text.contains("CC BY 4.0") || text.contains("CC-BY-4.0")
    };
    if !expected {
        return Err(Error::new(
            "license-review",
            "License declaration differs from original terms",
        ));
    }
    for (path, bytes) in payload.iter().filter(|(path, _)| license(path)) {
        let text = String::from_utf8_lossy(bytes).to_ascii_lowercase();
        if text.contains("distribute, sublicense, or transfer these materials")
            || text.contains("non-commercial")
            || text.contains("noncommercial")
        {
            return Err(Error::new(
                "license-review",
                format!("Redistribution restriction requires review: {path}"),
            ));
        }
    }
    Ok(())
}
fn prompt(bytes: &[u8]) -> Option<&str> {
    let text = std::str::from_utf8(bytes).ok()?;
    if text.contains("!`") || text.contains("${") || text.contains("<script") {
        return None;
    }
    Some(text)
}
fn frontmatter(text: &str) -> &str {
    if let Some(rest) = text.strip_prefix("---\n") {
        rest.split_once("\n---")
            .map(|(front, _)| front)
            .unwrap_or(text)
    } else {
        ""
    }
}
pub fn safe_update(
    previous: &Accepted,
    files: &[File],
    before: &BTreeMap<String, Vec<u8>>,
    after: &BTreeMap<String, Vec<u8>>,
) -> bool {
    if previous.files.len() != files.len() {
        return false;
    }
    previous.files.iter().zip(files).all(|(old, new)| {
        if old.path != new.path || old.mode != new.mode || old.output_path != new.output_path {
            return false;
        }
        if old.digest == new.digest {
            return true;
        }
        if license(&new.path) || !new.path.ends_with(".md") || new.mode != "100644" {
            return false;
        }
        let Some((old_text, new_text)) = before
            .get(&old.path)
            .and_then(|a| prompt(a))
            .zip(after.get(&new.path).and_then(|a| prompt(a)))
        else {
            return false;
        };
        // ponytail: fail closed on runtime-sensitive prompt tokens; this is not a security certification.
        frontmatter(old_text) == frontmatter(new_text)
            && ![
                "bash",
                "python",
                "npm",
                "npx",
                "pip",
                "curl",
                "wget",
                "http://",
                "https://",
                "scripts/",
                "allowed-tools",
                "command:",
                "powershell",
                "node ",
            ]
            .iter()
            .any(|token| {
                old_text.to_ascii_lowercase().contains(token)
                    || new_text.to_ascii_lowercase().contains(token)
            })
    })
}
