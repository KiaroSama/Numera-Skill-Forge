use crate::{Error, Result, validation};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub const MAX_FILE: u64 = 16 * 1024 * 1024;
pub const MAX_TOTAL: u64 = 256 * 1024 * 1024;
pub const MAX_FILES: usize = 10000;

pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn bytes(path: &Path) -> Result<Vec<u8>> {
    validation::reject_link(path)?;
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > MAX_FILE {
        return Err(Error::new("size-limit", "File exceeds 16 MiB"));
    }
    let mut result = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut result)?;
    if result.len() as u64 > MAX_FILE {
        return Err(Error::new("size-limit", "File grew beyond the input limit"));
    }
    Ok(result)
}
pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&bytes(path)?)?)
}
pub fn encoded<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}
pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    atomic(path, &encoded(value)?)
}
pub fn atomic(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::new("path", "Missing destination parent"))?;
    fs::create_dir_all(parent)?;
    validation::reject_link(parent)?;
    if path.exists() {
        validation::reject_link(path)?;
    }
    let temp = parent.join(format!(
        ".numera-write-{}-{}.tmp",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    use std::io::Write;
    let outcome = (|| -> Result<()> {
        let mut f = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        f.write_all(content)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    outcome
}

pub fn files(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut results = vec![];
    let mut seen = BTreeSet::new();
    let mut total = 0;
    walk(root, root, 0, &mut results, &mut seen, &mut total)?;
    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(results)
}
fn walk(
    root: &Path,
    dir: &Path,
    depth: usize,
    out: &mut Vec<(String, PathBuf)>,
    seen: &mut BTreeSet<String>,
    total: &mut u64,
) -> Result<()> {
    if depth > 32 {
        return Err(Error::new("depth-limit", "Directory nesting exceeds 32"));
    }
    validation::reject_link(dir)?;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if [
            ".git",
            ".ai",
            ".claude",
            ".codex",
            ".ignoreme",
            "node_modules",
            "target",
            ".forge-state",
            "secrets.md",
            "explain-AI.md",
        ]
        .iter()
        .any(|v| name == *v)
            || name.to_string_lossy().starts_with(".env")
        {
            continue;
        }
        validation::reject_link(&path)?;
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            walk(root, &path, depth + 1, out, seen, total)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| Error::new("escape", "File escaped input root"))?
                .components()
                .map(|v| {
                    v.as_os_str()
                        .to_str()
                        .ok_or_else(|| Error::new("encoding", "Path is not Unicode"))
                })
                .collect::<Result<Vec<_>>>()?
                .join("/");
            validation::relative_path(&relative)?;
            if !seen.insert(relative.to_lowercase()) {
                return Err(Error::new(
                    "case-collision",
                    "Input names collide on case-insensitive filesystems",
                ));
            }
            *total += metadata.len();
            if metadata.len() > MAX_FILE || *total > MAX_TOTAL || out.len() >= MAX_FILES {
                return Err(Error::new(
                    "size-limit",
                    "Snapshot file/byte limit exceeded",
                ));
            }
            out.push((relative, path));
        } else {
            return Err(Error::new("file-type", "Unsupported special input file"));
        }
    }
    Ok(())
}
