use crate::{Error, Result};
use std::path::{Component, Path, PathBuf};

pub fn identifier(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || value.starts_with('-')
        || value.ends_with('-')
    {
        return Err(Error::new(
            "invalid-id",
            "Use 1-64 lowercase ASCII letters, digits and internal hyphens",
        ));
    }
    Ok(())
}

pub fn relative_path(value: &str) -> Result<PathBuf> {
    if value.is_empty() || value.contains(['\\', ':', '\0']) || value.starts_with('/') {
        return Err(Error::new(
            "unsafe-path",
            "Expected a portable relative path",
        ));
    }
    for part in value.split('/') {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || "<>\"|?*".contains(c))
            || ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit())
        {
            return Err(Error::new(
                "unsafe-path",
                "Path contains traversal or a reserved platform name",
            ));
        }
    }
    let path = PathBuf::from(value);
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Error::new(
            "unsafe-path",
            "Path must contain only normal relative components",
        ));
    }
    Ok(path)
}

pub fn contained(root: &Path, value: &str) -> Result<PathBuf> {
    let relative = relative_path(value)?;
    let mut current = root.to_path_buf();
    reject_link(root)?;
    for part in relative.components() {
        current.push(part);
        if current.exists() {
            reject_link(&current)?;
        }
    }
    Ok(current)
}

pub fn reject_link(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(Error::new("unsafe-link", "Reparse points are not accepted"));
        }
    }
    if metadata.file_type().is_symlink() {
        return Err(Error::new(
            "unsafe-link",
            "Symbolic links require a separate verified import plan",
        ));
    }
    Ok(())
}

pub(crate) fn frontmatter(text: &str) -> Option<&str> {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))?;
    let mut end = 0;
    for raw in rest.split_inclusive('\n') {
        let line = raw
            .strip_suffix("\r\n")
            .or_else(|| raw.strip_suffix('\n'))
            .unwrap_or(raw);
        if line == "---" {
            return Some(&rest[..end]);
        }
        end += raw.len();
    }
    None
}
