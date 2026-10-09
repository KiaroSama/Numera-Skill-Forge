use crate::{
    Error, Result, discovery,
    model::{RefPolicy, Snapshot, Source},
    process, registry, storage, validation,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn candidate(workspace: &Path, source: &Source, offline: bool) -> Result<(Snapshot, PathBuf)> {
    registry::validate_repository(&source.repository)?;
    let cache = workspace.join(".forge-state/snapshots").join(&source.id);
    fs::create_dir_all(&cache)?;
    let origin = if let Some(local) = source.repository.strip_prefix("local:") {
        PathBuf::from(local)
    } else {
        let bare = workspace
            .join(".forge-state/git")
            .join(format!("{}.git", source.id));
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
            process::git(
                &[
                    "clone".into(),
                    "--bare".into(),
                    "--no-tags".into(),
                    "--depth=1".into(),
                    "--".into(),
                    source.repository.clone(),
                    bare.to_string_lossy().into_owned(),
                ],
                120,
            )?;
        }
        if !offline {
            process::git(
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
    let reference = match &source.reference {
        RefPolicy::DefaultBranch => "HEAD".into(),
        RefPolicy::Branch(s) => format!("refs/heads/{s}"),
        RefPolicy::Commit(s) => s.clone(),
        RefPolicy::Tag(s) => format!("refs/tags/{s}"),
        RefPolicy::LatestStableRelease => {
            return Err(Error::new(
                "release-provider",
                "Stable release selection requires an approved GitHub release provider; tags are not releases",
            ));
        }
    };
    if reference.starts_with('-') || reference.contains(['\r', '\n', '\0']) {
        return Err(Error::new("ref", "Invalid Git reference"));
    }
    let mut args = base.clone();
    args.extend([
        "rev-parse".into(),
        "--verify".into(),
        format!("{reference}^{{commit}}"),
    ]);
    let revision = String::from_utf8(process::git(&args, 30)?)
        .map_err(|_| Error::new("encoding", "Invalid commit identity"))?
        .trim()
        .to_string();
    if revision.len() != 40 || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::new("revision", "Expected immutable SHA-1 revision"));
    }
    let tree = cache.join(&revision);
    if !tree.exists() {
        let staging = cache.join(format!("stage-{}", std::process::id()));
        if staging.exists() {
            return Err(Error::new(
                "staging-conflict",
                "An interrupted snapshot exists; inspect recovery before retry",
            ));
        }
        fs::create_dir(&staging)?;
        let mut args = base.clone();
        args.extend([
            "ls-tree".into(),
            "-rz".into(),
            "--full-tree".into(),
            revision.clone(),
        ]);
        let listing = process::git(&args, 30)?;
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
            validation::relative_path(path)?;
            if fields.first() == Some(&"120000") || fields.first() == Some(&"160000") {
                return Err(Error::new(
                    "dependency-review",
                    "Symlinks/submodules require a separate import plan",
                ));
            }
            let selected = source.roots.is_empty()
                || source
                    .roots
                    .iter()
                    .any(|p| path == p || path.starts_with(&format!("{p}/")))
                || path.to_ascii_uppercase().contains("LICENSE")
                || path.to_ascii_uppercase().contains("COPYING");
            if !selected
                || source
                    .excludes
                    .iter()
                    .any(|p| path == p || path.starts_with(&format!("{p}/")))
            {
                continue;
            }
            let blob = fields
                .get(2)
                .ok_or_else(|| Error::new("git-tree", "Missing blob identity"))?;
            let mut a = base.clone();
            a.extend(["cat-file".into(), "blob".into(), (*blob).into()]);
            let bytes = process::git(&a, 30)?;
            count += 1;
            total += bytes.len() as u64;
            if bytes.len() as u64 > storage::MAX_FILE
                || total > storage::MAX_TOTAL
                || count > storage::MAX_FILES
            {
                return Err(Error::new("size-limit", "Snapshot limit exceeded"));
            }
            let target = validation::contained(&staging, path)?;
            storage::atomic(&target, &bytes)?;
            #[cfg(unix)]
            if fields.first() == Some(&"100755") {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&target, fs::Permissions::from_mode(0o755))?;
            }
        }
        fs::rename(staging, &tree)?;
    }
    let snapshot = discovery::inspect(&tree, &source.id, &revision)?;
    Ok((snapshot, tree))
}
