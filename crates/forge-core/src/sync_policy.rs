use crate::{Result, model::Snapshot, storage, validation};
use std::path::Path;

pub fn approved_content(root: &Path, before: &Snapshot, after: &Snapshot) -> Result<bool> {
    if before.license_digest != after.license_digest
        || before.files.len() != after.files.len()
        || storage::encoded(&before.components)? != storage::encoded(&after.components)?
    {
        return Ok(false);
    }
    let old = root
        .join(".forge-state/snapshots")
        .join(&before.source)
        .join(&before.revision);
    let new = root
        .join(".forge-state/snapshots")
        .join(&after.source)
        .join(&after.revision);
    for (a, b) in before.files.iter().zip(&after.files) {
        if a.path != b.path || a.executable != b.executable {
            return Ok(false);
        }
        if a.digest == b.digest {
            continue;
        }
        if !b.path.ends_with(".md")
            || crate::model::runtime_payload(b)
            || b.path.to_ascii_uppercase().contains("LICENSE")
            || b.path.to_ascii_uppercase().contains("NOTICE")
        {
            return Ok(false);
        }
        let old_bytes = storage::bytes(&validation::contained(&old, &a.path)?)?;
        let new_bytes = storage::bytes(&validation::contained(&new, &b.path)?)?;
        let (Ok(old), Ok(new)) = (
            std::str::from_utf8(&old_bytes),
            std::str::from_utf8(&new_bytes),
        ) else {
            return Ok(false);
        };
        if validation::frontmatter(old) != validation::frontmatter(new)
            || ((old.starts_with("---") || new.starts_with("---"))
                && (validation::frontmatter(old).is_none()
                    || validation::frontmatter(new).is_none()))
        {
            return Ok(false);
        }
        // Fail closed on execution/network-sensitive prompts; this is not a safety certification.
        for text in [old, new] {
            let lower = text.to_ascii_lowercase();
            if [
                "!`",
                "${",
                "<script",
                "bash",
                "python",
                "powershell",
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
            ]
            .iter()
            .any(|token| lower.contains(token))
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
