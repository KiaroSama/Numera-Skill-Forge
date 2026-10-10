use crate::{
    Error, Result, discovery,
    model::{Snapshot, Source},
    storage, validation,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[path = "sources/authentication.rs"]
mod authentication;
#[path = "sources/providers.rs"]
pub(crate) mod providers;
#[path = "sources/refs.rs"]
mod refs;

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        if self.0.exists() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn protected(path: &str) -> bool {
    let parts = path
        .split('/')
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    parts.iter().any(|part| {
        matches!(
            part.as_str(),
            ".git"
                | ".ai"
                | ".ignoreme"
                | "secrets.md"
                | "explain-ai.md"
                | ".forge-state"
                | ".specify"
                | ".codebase-memory"
                | "graphify-out"
        ) || part.starts_with(".env")
    }) || parts.windows(2).any(|pair| {
        matches!(
            pair[0].as_str(),
            ".claude" | ".codex" | ".kiro" | ".cursor" | ".cline" | ".agents"
        ) && matches!(
            pair[1].as_str(),
            "auth.json"
                | "credentials.json"
                | ".credentials.json"
                | "settings.local.json"
                | "sessions"
                | "history.jsonl"
                | "cache"
                | "logs"
        )
    })
}

pub fn candidate(workspace: &Path, source: &Source, offline: bool) -> Result<(Snapshot, PathBuf)> {
    providers::acquisition(source, offline)?;
    validation::identifier(&source.id)?;
    validation::reject_link(workspace)?;
    let state = workspace.join(".forge-state");
    if state.exists() {
        validation::reject_link(&state)?;
    }
    fs::create_dir_all(&state)?;
    let transport = providers::Transport::new(&state, source, offline)?;
    let cache = validation::contained(&state, &format!("snapshots/{}", source.id))?;
    fs::create_dir_all(&cache)?;
    let origin = if let Some(local) = source.repository.strip_prefix("local:") {
        PathBuf::from(local)
    } else {
        let bare = validation::contained(&state, &format!("git/{}.git", source.id))?;
        if !bare.exists() {
            if offline {
                return Err(Error::new(
                    "offline",
                    "No cached repository; check online first",
                ));
            }
            fs::create_dir_all(
                bare.parent()
                    .ok_or_else(|| Error::new("path", "Missing cache parent"))?,
            )?;
            transport.git(
                &[
                    "-C".into(),
                    bare.parent()
                        .ok_or_else(|| Error::new("path", "Missing cache parent"))?
                        .to_string_lossy()
                        .into_owned(),
                    "clone".into(),
                    "--bare".into(),
                    "--no-tags".into(),
                    "--depth=1".into(),
                    "--".into(),
                    source.repository.clone(),
                    bare.file_name()
                        .ok_or_else(|| Error::new("path", "Missing cache name"))?
                        .to_string_lossy()
                        .into_owned(),
                ],
                120,
            )?;
        }
        transport.verify_cache(&bare, &source.repository)?;
        let origin = String::from_utf8(transport.git(
            &[
                "--git-dir".into(),
                bare.to_string_lossy().into_owned(),
                "remote".into(),
                "get-url".into(),
                "origin".into(),
            ],
            30,
        )?)
        .map_err(|_| Error::new("encoding", "Repository origin is not UTF-8"))?;
        if origin.trim() != source.repository {
            return Err(Error::new(
                "cache-origin",
                "Cached repository origin differs from registered repository; retain accepted inputs and select a new source ID",
            ));
        }
        if !offline {
            transport.git(
                &[
                    "--git-dir".into(),
                    bare.to_string_lossy().into_owned(),
                    "fetch".into(),
                    "--force".into(),
                    "--prune".into(),
                    "origin".into(),
                    "+refs/heads/*:refs/heads/*".into(),
                    "+refs/tags/*:refs/tags/*".into(),
                ],
                120,
            )?;
        }
        bare
    };
    validation::reject_link(&origin)?;
    let base = vec!["-C".to_string(), origin.to_string_lossy().into_owned()];
    let default_ref = cache.join("default-reference.json");
    let reference = if offline
        && matches!(source.reference, crate::model::RefPolicy::DefaultBranch)
        && default_ref.exists()
    {
        storage::read::<String>(&default_ref)?
    } else {
        refs::resolve(source, &origin, offline, &transport)?
    };
    if !offline && matches!(source.reference, crate::model::RefPolicy::DefaultBranch) {
        storage::write(&default_ref, &reference)?;
    }
    if matches!(source.reference, crate::model::RefPolicy::Commit(_))
        && !source.repository.starts_with("local:")
    {
        let mut check = base.clone();
        check.extend([
            "cat-file".into(),
            "-e".into(),
            format!("{reference}^{{commit}}"),
        ]);
        if transport.git(&check, 30).is_err() {
            if offline {
                return Err(Error::new("offline", "Pinned revision is not cached"));
            }
            let mut fetch = base.clone();
            fetch.extend([
                "fetch".into(),
                "--depth=1".into(),
                "--no-tags".into(),
                "origin".into(),
                reference.clone(),
            ]);
            transport.git(&fetch, 120)?;
        }
    }
    if reference.starts_with('-') || reference.contains(['\r', '\n', '\0']) {
        return Err(Error::new("ref", "Invalid Git reference"));
    }
    let mut args = base.clone();
    args.extend([
        "rev-parse".into(),
        "--verify".into(),
        format!("{reference}^{{commit}}"),
    ]);
    let revision = String::from_utf8(transport.git(&args, 30)?)
        .map_err(|_| Error::new("encoding", "Invalid commit identity"))?
        .trim()
        .to_string();
    if revision.len() != 40 || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::new("revision", "Expected immutable SHA-1 revision"));
    }
    let tree = cache.join(&revision);
    let selection = storage::digest(&storage::encoded(&(
        &source.repository,
        &source.roots,
        &source.excludes,
    ))?);
    let selection_path = cache.join(format!("{revision}.selection.json"));
    if tree.exists()
        && selection_path.exists()
        && storage::read::<String>(&selection_path)? != selection
    {
        return Err(Error::new(
            "snapshot-selection",
            "Cached snapshot selection changed or predates selection metadata; retain accepted snapshot and use a new source ID",
        ));
    }
    let mut modes = std::collections::BTreeMap::new();
    if !tree.exists() || !selection_path.exists() {
        let staging = cache.join(format!("stage-{}", crate::transaction::operation_id()));
        fs::create_dir(&staging)?;
        let _cleanup = Staging(staging.clone());
        let mut args = base.clone();
        args.extend([
            "ls-tree".into(),
            "-rz".into(),
            "--full-tree".into(),
            revision.clone(),
        ]);
        let listing = transport.git(&args, 30)?;
        let mut count = 0;
        let mut total = 0;
        for record in listing.split(|b| *b == 0).filter(|b| !b.is_empty()) {
            let tab = record
                .iter()
                .position(|b| *b == b'\t')
                .ok_or_else(|| Error::new("git-tree", "Invalid tree record"))?;
            let header = std::str::from_utf8(&record[..tab])
                .map_err(|_| Error::new("encoding", "Invalid tree header"))?;
            let fields = header.split_whitespace().collect::<Vec<_>>();
            let path = std::str::from_utf8(&record[tab + 1..])
                .map_err(|_| Error::new("encoding", "Non-Unicode paths are unsupported"))?;
            if protected(path) {
                continue;
            }
            let selected = source.roots.is_empty()
                || source
                    .roots
                    .iter()
                    .any(|p| path == p || path.starts_with(&format!("{p}/")))
                || path.to_ascii_uppercase().contains("LICENSE")
                || path.to_ascii_uppercase().contains("COPYING")
                || path.to_ascii_uppercase().contains("NOTICE");
            if !selected
                || source
                    .excludes
                    .iter()
                    .any(|p| path == p || path.starts_with(&format!("{p}/")))
            {
                continue;
            }
            validation::relative_path(path)?;
            if fields.len() != 3 || !matches!(fields[0], "100644" | "100755") {
                return Err(Error::new(
                    "dependency-review",
                    "Selected symlink/submodule requires pinned import review",
                ));
            }
            let blob = fields
                .get(2)
                .ok_or_else(|| Error::new("git-tree", "Missing blob identity"))?;
            let mut a = base.clone();
            a.extend(["cat-file".into(), "blob".into(), (*blob).into()]);
            let bytes = transport.git(&a, 30)?;
            count += 1;
            total += bytes.len() as u64;
            if bytes.len() as u64 > storage::MAX_FILE
                || total > storage::MAX_TOTAL
                || count > storage::MAX_FILES
            {
                return Err(Error::new("size-limit", "Snapshot limit exceeded"));
            }
            modes.insert(path.to_owned(), fields[0] == "100755");
            let target = validation::contained(&staging, path)?;
            storage::atomic(&target, &bytes)?;
            #[cfg(unix)]
            if fields.first() == Some(&"100755") {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&target, fs::Permissions::from_mode(0o755))?;
            }
        }
        let _ = discovery::inspect(&staging, &source.id, &revision)?;
        crate::jobs::checkpoint()?;
        if tree.exists() {
            let old = storage::payload_files(&tree)?;
            let new = storage::payload_files(&staging)?
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>();
            let mut matches = old.len() == new.len();
            for (path, physical) in old {
                let Some(fresh) = new.get(&path) else {
                    matches = false;
                    break;
                };
                if storage::bytes(&physical)? != storage::bytes(fresh)? {
                    matches = false;
                    break;
                }
            }
            if !matches {
                return Err(Error::new(
                    "snapshot-selection",
                    "Legacy snapshot differs from immutable selected payload; accepted inputs retained, select a new source ID",
                ));
            }
        } else {
            fs::rename(&staging, &tree)?;
        }
        storage::write(&cache.join(format!("{revision}.modes.json")), &modes)?;
        storage::write(&selection_path, &selection)?;
    }
    if modes.is_empty() && cache.join(format!("{revision}.modes.json")).exists() {
        modes = storage::read(&cache.join(format!("{revision}.modes.json")))?;
    }
    let mut snapshot = discovery::inspect(&tree, &source.id, &revision)?;
    for file in &mut snapshot.files {
        let mode = modes.get(&file.path).ok_or_else(|| Error::new("snapshot-modes", "Cached immutable mode evidence missing; preserve accepted state and rediscover using a new source identity"))?;
        file.executable = *mode;
    }
    snapshot.content_digest = storage::digest(&storage::encoded(&snapshot.files)?);
    Ok((snapshot, tree))
}
