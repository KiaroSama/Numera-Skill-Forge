use super::model::{Accepted, File, Skill};
use crate::{Error, Result, storage, validation};
use std::collections::BTreeMap;

pub fn validate_skill(skill: &Skill) -> Result<()> {
    validation::identifier(&skill.id)?;
    validation::relative_path(&skill.root)?;
    validation::relative_path(&skill.license_path)?;
    if !matches!(skill.license.as_str(), "MIT" | "Apache-2.0")
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
pub fn verify_license(skill: &Skill, payload: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    let bytes = payload
        .get(&skill.license_path)
        .ok_or_else(|| Error::new("license-review", "Missing reviewed license file"))?;
    if storage::digest(bytes) != skill.license_digest {
        return Err(Error::new(
            "license-review",
            "License differs from reviewed identity",
        ));
    }
    let text = String::from_utf8_lossy(bytes);
    let expected = if skill.license == "MIT" {
        text.contains("Permission is hereby granted")
    } else {
        text.contains("Apache License") && text.contains("Version 2.0")
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
