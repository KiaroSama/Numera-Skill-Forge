use super::model::Collection;
use crate::{Error, Result, process, registry, storage, validation};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub struct Repository {
    path: PathBuf,
}
#[derive(Clone)]
pub struct Blob {
    pub path: String,
    pub oid: String,
    pub mode: String,
}
impl Repository {
    pub fn open(root: &Path, collection: &Collection, offline: bool) -> Result<Self> {
        registry::validate_repository(&collection.repository)?;
        let path = if let Some(path) = collection.repository.strip_prefix("local:") {
            PathBuf::from(path)
        } else {
            let path = root
                .join(".forge-state/collections/git")
                .join(format!("{}.git", collection.id));
            if !path.exists() {
                if offline {
                    return Err(Error::new("offline", "Collection repository is not cached"));
                }
                fs::create_dir_all(
                    path.parent()
                        .ok_or_else(|| Error::new("path", "Missing cache parent"))?,
                )?;
                process::git(
                    &[
                        "clone".into(),
                        "--bare".into(),
                        "--depth=1".into(),
                        "--no-tags".into(),
                        "--".into(),
                        collection.repository.clone(),
                        path.to_string_lossy().into_owned(),
                    ],
                    120,
                )?;
            }
            path
        };
        validation::reject_link(&path)?;
        let repository = Self { path };
        if !offline && !collection.repository.starts_with("local:") {
            repository.command(
                &[
                    "fetch",
                    "--depth=1",
                    "--no-tags",
                    "origin",
                    &format!(
                        "+refs/heads/{}:refs/heads/{}",
                        collection.branch, collection.branch
                    ),
                ],
                120,
            )?;
        }
        Ok(repository)
    }
    pub fn command(&self, args: &[&str], timeout: u64) -> Result<Vec<u8>> {
        let mut command = vec!["-C".into(), self.path.to_string_lossy().into_owned()];
        command.extend(args.iter().map(|arg| arg.to_string()));
        process::git(&command, timeout)
    }
    pub fn revision(&self, reference: &str) -> Result<String> {
        if reference.starts_with('-') || reference.contains(['\0', '\r', '\n']) {
            return Err(Error::new("ref", "Invalid collection reference"));
        }
        let value = String::from_utf8(self.command(
            &["rev-parse", "--verify", &format!("{reference}^{{commit}}")],
            30,
        )?)
        .map_err(|_| Error::new("encoding", "Invalid revision"))?
        .trim()
        .to_owned();
        if value.len() != 40 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::new("revision", "Expected immutable commit"));
        }
        Ok(value)
    }
    pub fn ensure_revision(&self, revision: &str, offline: bool) -> Result<()> {
        if revision.len() != 40 || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::new("revision", "Invalid accepted revision"));
        }
        if self.revision(revision).is_err() {
            if offline {
                return Err(Error::new("offline", "Accepted revision is not cached"));
            }
            self.command(
                &["fetch", "--depth=1", "--no-tags", "origin", revision],
                120,
            )?;
        }
        Ok(())
    }
    pub fn tree(&self, revision: &str) -> Result<Vec<Blob>> {
        let listing = self.command(&["ls-tree", "-rz", "--full-tree", revision], 60)?;
        let mut result = vec![];
        for record in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let tab = record
                .iter()
                .position(|b| *b == b'\t')
                .ok_or_else(|| Error::new("git-tree", "Malformed tree"))?;
            let header = std::str::from_utf8(&record[..tab])
                .map_err(|_| Error::new("encoding", "Malformed tree header"))?;
            let fields = header.split_whitespace().collect::<Vec<_>>();
            let path = std::str::from_utf8(&record[tab + 1..])
                .map_err(|_| Error::new("encoding", "Non UTF-8 tree path"))?;
            if fields.len() != 3 {
                return Err(Error::new("git-tree", "Malformed tree fields"));
            }
            result.push(Blob {
                path: path.into(),
                oid: fields[2].into(),
                mode: fields[0].into(),
            });
        }
        Ok(result)
    }
    pub fn blobs(&self, blobs: &[Blob]) -> Result<BTreeMap<String, Vec<u8>>> {
        if blobs.len() > storage::MAX_FILES {
            return Err(Error::new("size-limit", "Collection has too many files"));
        }
        for blob in blobs {
            validation::relative_path(&blob.path)?;
            if !matches!(blob.mode.as_str(), "100644" | "100755") {
                return Err(Error::new(
                    "dependency-review",
                    "Selected symlink/submodule requires review",
                ));
            }
            if blob.oid.len() != 40 || !blob.oid.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(Error::new("git-tree", "Invalid blob identity"));
            }
        }
        // One argument-safe batch avoids a process per companion file. No checkout/filter execution.
        let input = blobs
            .iter()
            .map(|b| format!("{}\n", b.oid))
            .collect::<String>()
            .into_bytes();
        let output = process::git_input(
            &[
                "-C".into(),
                self.path.to_string_lossy().into_owned(),
                "cat-file".into(),
                "--batch".into(),
            ],
            180,
            &[],
            Some(input),
        )?;
        let mut cursor = 0usize;
        let mut result = BTreeMap::new();
        let mut total = 0u64;
        for blob in blobs {
            let end = output[cursor..]
                .iter()
                .position(|b| *b == b'\n')
                .ok_or_else(|| Error::new("git-blob", "Missing batch header"))?
                + cursor;
            let header = std::str::from_utf8(&output[cursor..end])
                .map_err(|_| Error::new("git-blob", "Invalid batch header"))?;
            let parts = header.split_whitespace().collect::<Vec<_>>();
            if parts.len() != 3 || parts[0] != blob.oid || parts[1] != "blob" {
                return Err(Error::new("git-blob", "Unexpected batch object"));
            }
            let size = parts[2]
                .parse::<usize>()
                .map_err(|_| Error::new("git-blob", "Invalid batch size"))?;
            total += size as u64;
            if size as u64 > storage::MAX_FILE || total > storage::MAX_TOTAL {
                return Err(Error::new("size-limit", "Collection exceeds byte limits"));
            }
            cursor = end + 1;
            let end = cursor
                .checked_add(size)
                .ok_or_else(|| Error::new("size-limit", "Batch overflow"))?;
            if end >= output.len() || output[end] != b'\n' {
                return Err(Error::new("git-blob", "Truncated batch object"));
            }
            let bytes = output[cursor..end].to_vec();
            cursor = end + 1;
            if bytes.starts_with(b"version https://git-lfs.github.com/spec/") {
                return Err(Error::new(
                    "lfs-unresolved",
                    "Selected LFS payload requires review",
                ));
            }
            result.insert(blob.path.clone(), bytes);
        }
        if cursor != output.len() {
            return Err(Error::new("git-blob", "Unexpected trailing batch data"));
        }
        Ok(result)
    }
}
