mod git;
use git::Git;
mod github;
use crate::{Error, Result, storage, transaction, validation};
use github::{pr, push, verify_remote};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PublishOptions {
    pub mode: String,
    pub expected_remote: Option<String>,
    pub expected_head: Option<String>,
    pub expected_remote_head: Option<String>,
    pub expected_account: Option<String>,
    pub publication_branch: Option<String>,
    pub author_name: Option<String>,
    pub author_email: Option<String>,
    /// Only a bare repository physically inside this workspace; never arbitrary file URLs.
    #[serde(default)]
    pub allow_owned_local_remote: bool,
}

/// Compatibility entry point: mutation callers must supply the full reviewed options contract.
pub fn publish(
    root: &Path,
    mode: &str,
    expected_remote: Option<&str>,
    author_email: Option<&str>,
) -> Result<Value> {
    publish_with_options(
        root,
        &PublishOptions {
            mode: mode.into(),
            expected_remote: expected_remote.map(str::to_owned),
            author_email: author_email.map(str::to_owned),
            ..Default::default()
        },
    )
}

struct IndexGuard {
    scratch: PathBuf,
    lock: Option<PathBuf>,
}
impl Drop for IndexGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.scratch);
        let _ = fs::remove_file(self.scratch.with_extension("index.lock"));
        if let Some(lock) = &self.lock {
            let _ = fs::remove_file(lock);
        }
    }
}
pub(super) fn required<'a>(value: &'a Option<String>, name: &str) -> Result<&'a str> {
    value.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| {
        Error::new(
            "publication-review",
            format!("Explicit {name} is required before publication mutation"),
        )
    })
}
pub(super) fn object_id(value: &str) -> Result<()> {
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|v| v.is_ascii_hexdigit()) {
        return Err(Error::new(
            "publication-input",
            "Expected a full Git object identity",
        ));
    }
    Ok(())
}
fn owned(path: &str) -> bool {
    path == "forge.lock.json"
        || path == ".claude-plugin/marketplace.json"
        || ["registry/", "packages/", "catalogs/"]
            .iter()
            .any(|p| path.starts_with(p))
}
fn protected(path: &str) -> bool {
    path.split('/').any(|part| {
        let p = part.to_ascii_lowercase();
        matches!(
            p.as_str(),
            ".git"
                | ".ai"
                | ".specify"
                | "specs"
                | ".ignoreme"
                | "secrets.md"
                | "explain-ai.md"
                | "reference.md"
                | "claude.md"
                | "agents.md"
                | "settings.local.json"
                | "auth.json"
                | "credentials.json"
                | "node_modules"
                | ".forge-state"
        ) || (p.starts_with(".env") && p != ".env.example")
    })
}
fn path_list(bytes: Vec<u8>) -> Result<Vec<String>> {
    bytes
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| {
            String::from_utf8(p.to_vec())
                .map_err(|_| Error::new("encoding", "Publication path is not UTF-8"))
        })
        .collect()
}
fn content_check(bytes: &[u8]) -> Result<()> {
    let text = String::from_utf8_lossy(bytes);
    if [
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----",
    ]
    .iter()
    .any(|p| text.contains(p))
        || text
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|w| {
                (w.starts_with("ghp_") && w.len() >= 36)
                    || (w.starts_with("github_pat_") && w.len() >= 50)
            })
    {
        return Err(Error::new(
            "protected-publication",
            "Recognizable credential material cannot be published",
        ));
    }
    Ok(())
}
fn files(root: &Path, git: &Git<'_>) -> Result<BTreeMap<String, Option<Vec<u8>>>> {
    let mut paths = BTreeSet::new();
    // Track removals as well as current files; never enumerate or stage unrelated roots.
    for path in path_list(git.run(&["ls-tree", "-r", "--name-only", "-z", "HEAD"], None)?)? {
        if owned(&path) {
            paths.insert(path);
        }
    }
    for prefix in ["registry", "packages", "catalogs"] {
        let directory = root.join(prefix);
        if directory.exists() {
            for (path, _) in storage::payload_files(&directory)? {
                paths.insert(format!("{prefix}/{path}"));
            }
        }
    }
    for path in ["forge.lock.json", ".claude-plugin/marketplace.json"] {
        if root.join(path).exists() {
            paths.insert(path.into());
        }
    }
    let mut result = BTreeMap::new();
    let mut total = 0u64;
    for path in paths {
        validation::relative_path(&path)?;
        if protected(&path) {
            return Err(Error::new(
                "protected-publication",
                "Protected local-only paths cannot be included in publication",
            ));
        }
        let physical = validation::contained(root, &path)?;
        let bytes = if physical.exists() {
            let bytes = storage::bytes(&physical)?;
            content_check(&bytes)?;
            total += bytes.len() as u64;
            Some(bytes)
        } else {
            None
        };
        if total > storage::MAX_TOTAL || result.len() >= storage::MAX_FILES {
            return Err(Error::new(
                "size-limit",
                "Publication exceeds bounded file/byte limits",
            ));
        }
        result.insert(path, bytes);
    }
    Ok(result)
}
fn modes(
    root: &Path,
    payload: &BTreeMap<String, Option<Vec<u8>>>,
) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    let mut add = |path: String, value: u64| -> Result<()> {
        validation::relative_path(&path)?;
        if !owned(&path) || protected(&path) || !payload.contains_key(&path) {
            return Err(Error::new(
                "publication-mode",
                "Mode metadata points outside current publication payload",
            ));
        }
        let mode = match value {
            0o644 | 100644 | 0o100644 => "100644",
            0o755 | 100755 | 0o100755 => "100755",
            _ => {
                return Err(Error::new(
                    "publication-mode",
                    "Unsupported published Git mode",
                ));
            }
        };
        if result
            .insert(path, mode.into())
            .is_some_and(|old| old != mode)
        {
            return Err(Error::new(
                "publication-mode",
                "Conflicting original mode metadata",
            ));
        }
        Ok(())
    };
    for (path, bytes) in payload {
        if path.starts_with("packages/")
            && path.ends_with("/provenance.json")
            && let Some(bytes) = bytes
        {
            let value: Value = serde_json::from_slice(bytes)?;
            if let Some(map) = value["file_modes"].as_object() {
                let prefix = path.strip_suffix("provenance.json").unwrap_or("");
                for (name, mode) in map {
                    add(
                        format!("{prefix}{name}"),
                        mode.as_u64().ok_or_else(|| {
                            Error::new("publication-mode", "Invalid mode metadata")
                        })?,
                    )?;
                }
            }
        }
    }
    if root.join("catalogs/collections.lock.json").exists() {
        let value: Value = storage::read(&root.join("catalogs/collections.lock.json"))?;
        if let Some(entries) = value["entries"].as_object() {
            for entry in entries.values() {
                if let Some(entries) = entry["files"].as_array() {
                    for file in entries {
                        let path = file["output_path"].as_str().ok_or_else(|| {
                            Error::new("publication-mode", "Missing collection output path")
                        })?;
                        let value = file["mode"].as_str().ok_or_else(|| {
                            Error::new("publication-mode", "Missing collection Git mode")
                        })?;
                        add(
                            path.into(),
                            value.parse().map_err(|_| {
                                Error::new("publication-mode", "Invalid collection Git mode")
                            })?,
                        )?;
                    }
                }
            }
        }
    }
    Ok(result)
}
fn verify_head(git: &Git<'_>, branch: &str, expected: &str) -> Result<()> {
    if git.text(&["rev-parse", "HEAD"])? != expected
        || git.text(&["branch", "--show-current"])? != branch
    {
        return Err(Error::new(
            "head-advanced",
            "Local HEAD or checked-out branch changed; review again",
        ));
    }
    Ok(())
}
fn verify_payload(root: &Path, payload: &BTreeMap<String, Option<Vec<u8>>>) -> Result<()> {
    for (path, expected) in payload {
        let physical = validation::contained(root, path)?;
        let current = if physical.exists() {
            Some(storage::bytes(&physical)?)
        } else {
            None
        };
        if &current != expected {
            return Err(Error::new(
                "publication-conflict",
                "Intended file bytes changed during publication; retry after review",
            ));
        }
    }
    Ok(())
}
fn index_info(
    git: &Git<'_>,
    payload: &BTreeMap<String, Option<Vec<u8>>>,
    hints: &BTreeMap<String, String>,
    head: &str,
) -> Result<Vec<u8>> {
    let tree = git.run(&["ls-tree", "-r", "-z", head], None)?;
    let mut prior = BTreeMap::new();
    for entry in tree.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let text = std::str::from_utf8(entry)
            .map_err(|_| Error::new("encoding", "Git tree path is not UTF-8"))?;
        if let Some((metadata, path)) = text.split_once('\t') {
            prior.insert(
                path.to_owned(),
                metadata.split_whitespace().next().unwrap_or("").to_owned(),
            );
        }
    }
    let input = payload
        .iter()
        .filter(|(_, bytes)| bytes.is_some())
        .map(|(path, _)| format!("\"{path}\"\n"))
        .collect::<String>();
    // One bounded raw-byte batch: no clean filters or checkout code, even for large collections.
    let hashes = git.text_input(
        &["hash-object", "--no-filters", "-w", "--stdin-paths"],
        input.into_bytes(),
    )?;
    let mut hashes = hashes.lines();
    let mut info = Vec::new();
    for (path, bytes) in payload {
        if bytes.is_some() {
            let oid = hashes.next().ok_or_else(|| {
                Error::new("publication-hash", "Raw hash batch omitted a payload file")
            })?;
            object_id(oid)?;
            let mode = hints.get(path).cloned().unwrap_or_else(|| {
                prior
                    .get(path)
                    .filter(|m| m.as_str() == "100755")
                    .cloned()
                    .unwrap_or_else(|| "100644".into())
            });
            info.extend_from_slice(format!("{mode} {oid}\t{path}\0").as_bytes());
        } else {
            info.extend_from_slice(format!("0 {}\t{path}\0", "0".repeat(head.len())).as_bytes());
        }
    }
    Ok(info)
}
fn raw_identity(git: &Git<'_>, commit: &str, email: &str) -> Result<()> {
    let raw = git.text(&["cat-file", "commit", commit])?;
    for prefix in ["author ", "committer "] {
        if !raw
            .lines()
            .find(|l| l.starts_with(prefix))
            .is_some_and(|l| l.contains(&format!("<{email}>")))
        {
            return Err(Error::new(
                "publication-identity",
                "Outgoing raw commit identity differs from reviewed public email",
            ));
        }
    }
    Ok(())
}

pub fn publish_with_options(root: &Path, options: &PublishOptions) -> Result<Value> {
    if !["preview", "local", "commit", "direct-push", "pull-request"]
        .contains(&options.mode.as_str())
    {
        return Err(Error::new(
            "publish-mode",
            "Expected preview/local/commit/pull-request/direct-push",
        ));
    }
    validation::reject_link(root)?;
    let root = fs::canonicalize(root)?;
    let root_text = root
        .to_str()
        .ok_or_else(|| Error::new("encoding", "Workspace path is not Unicode"))?
        .to_owned();
    let git = Git {
        root: root_text.clone(),
        index: None,
        options,
    };
    let branch = git.text(&["branch", "--show-current"])?;
    let head = git.text(&["rev-parse", "HEAD"])?;
    let payload = files(&root, &git)?;
    if matches!(options.mode.as_str(), "preview" | "local") {
        return Ok(
            json!({"mode":options.mode,"branch":branch,"head":head,"owned_paths":payload.keys().collect::<Vec<_>>(),"published":false,"status":"preview"}),
        );
    }
    if branch.is_empty() {
        return Err(Error::new(
            "publication-branch",
            "Publication requires an explicit checked-out branch, not detached HEAD",
        ));
    }
    let expected = required(&options.expected_head, "expected_head")?;
    object_id(expected)?;
    verify_head(&git, &branch, expected)?;
    let email = required(&options.author_email, "author_email")?;
    if email != "Kiaro.Sama.Dev@gmail.com" {
        return Err(Error::new(
            "publication-identity",
            "Configure the approved public author email before publication",
        ));
    }
    let name = required(&options.author_name, "author_name")?;
    if name.contains(['\r', '\n', '\0', '<', '>']) {
        return Err(Error::new(
            "publication-identity",
            "Invalid public author name",
        ));
    }
    let remote_mode = matches!(options.mode.as_str(), "direct-push" | "pull-request");
    if options.expected_remote.is_some()
        && !remote_mode
        && git.text(&["remote", "get-url", "origin"])?
            != required(&options.expected_remote, "expected_remote")?
    {
        return Err(Error::new(
            "remote-mismatch",
            "Actual origin differs from reviewed repository",
        ));
    }
    let repository = if remote_mode {
        verify_remote(&root, &git, options, &branch)?
    } else {
        None
    };
    if remote_mode && options.expected_remote_head.as_deref() != Some(head.as_str()) {
        return Err(Error::new(
            "publication-history",
            "Remote publication requires local base exactly equal reviewed remote base; do not publish unrelated outgoing history",
        ));
    }
    let pr_branch = if options.mode == "pull-request" {
        if repository.is_none() {
            return Err(Error::new(
                "publication-pr",
                "PR creation requires GitHub, not a bare local test remote",
            ));
        }
        let value = required(&options.publication_branch, "publication_branch")?;
        if !value.starts_with("chore/")
            || value == branch
            || git
                .run(&["check-ref-format", "--branch", value], None)
                .is_err()
        {
            return Err(Error::new(
                "publication-branch",
                "PR requires an explicit valid chore/ publication branch different from base",
            ));
        }
        Some(value)
    } else {
        None
    };
    let _writer = transaction::Writer::acquire(&root)?;
    let hints = modes(&root, &payload)?;
    // An unrelated staged/unstaged file is preserved; an owned staged conflict requires review.
    for path in path_list(git.run(&["diff", "--cached", "--name-only", "-z"], None)?)? {
        if owned(&path) {
            return Err(Error::new(
                "staged-conflict",
                "Existing staged intended-path changes require review; unrelated staged work is preserved",
            ));
        }
    }
    let state = root.join(".forge-state");
    let scratch = state.join(format!("publication-{}.index", transaction::operation_id()));
    let mut cleanup = IndexGuard {
        scratch: scratch.clone(),
        lock: None,
    };
    let isolated = Git {
        root: root_text,
        index: Some(scratch.to_string_lossy().into_owned()),
        options,
    };
    isolated.run(&["read-tree", &head], None)?;
    let info = index_info(&isolated, &payload, &hints, &head)?;
    isolated.run(&["update-index", "-z", "--index-info"], Some(info.clone()))?;
    let tree = isolated.text(&["write-tree"])?;
    if tree == git.text(&["rev-parse", "HEAD^{tree}"])? {
        return Ok(json!({"mode":options.mode,"status":"unchanged","published":false,"head":head}));
    }
    verify_payload(&root, &payload)?;
    verify_head(&git, &branch, &head)?;
    let commit = isolated.text_input(
        &["commit-tree", &tree, "-p", &head],
        b"chore: update packages\n\nCo-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n".to_vec(),
    )?;
    raw_identity(&git, &commit, email)?;
    if let Some(pr_branch) = pr_branch {
        let receipt_path = state.join("publication.json");
        let remote_branch = git.text(&[
            "ls-remote",
            "--refs",
            "origin",
            &format!("refs/heads/{pr_branch}"),
        ])?;
        let previous: Option<Value> = if receipt_path.exists() {
            Some(storage::read(&receipt_path)?)
        } else {
            None
        };
        let reuse = previous.as_ref().filter(|p| {
            p["repository"].as_str() == repository.as_deref()
                && p["branch"] == pr_branch
                && p["base"] == branch
                && p["base_head"] == head
                && p["tree"] == tree
        });
        let published_commit = if remote_branch.is_empty() {
            reuse
                .and_then(|record| record["commit"].as_str())
                .unwrap_or(&commit)
                .to_owned()
        } else if let Some(record) = reuse {
            let old = record["commit"]
                .as_str()
                .ok_or_else(|| Error::new("publication-conflict", "Invalid publication receipt"))?;
            if remote_branch.split_whitespace().next() != Some(old) {
                return Err(Error::new(
                    "remote-advanced",
                    "PR branch advanced; retain remote work and review again",
                ));
            }
            old.into()
        } else {
            return Err(Error::new(
                "publication-conflict",
                "Publication branch already exists without exact owned receipt; choose a new explicit branch",
            ));
        };
        verify_remote(&root, &git, options, &branch)?;
        verify_payload(&root, &payload)?;
        verify_head(&git, &branch, &head)?;
        let local = git
            .run(
                &["rev-parse", "--verify", &format!("refs/heads/{pr_branch}")],
                None,
            )
            .ok();
        if let Some(local) = &local {
            let local = std::str::from_utf8(local)
                .map_err(|_| Error::new("encoding", "Local branch identity is not UTF-8"))?;
            if reuse.is_none() || local.trim() != published_commit {
                return Err(Error::new(
                    "publication-conflict",
                    "Local publication branch differs from exact owned receipt",
                ));
            }
        }
        storage::write(
            &receipt_path,
            &json!({"repository":repository,"base":branch,"branch":pr_branch,"base_head":head,"tree":tree,"commit":published_commit}),
        )?;
        if local.is_none() {
            git.run(
                &[
                    "update-ref",
                    &format!("refs/heads/{pr_branch}"),
                    &published_commit,
                    &"0".repeat(head.len()),
                ],
                None,
            )?;
        }
        if remote_branch.is_empty() {
            push(&git, &published_commit, pr_branch)?;
        }
        storage::write(
            &receipt_path,
            &json!({"repository":repository,"base":branch,"branch":pr_branch,"base_head":head,"tree":tree,"commit":published_commit}),
        )?;
        let pull_request = pr(repository.as_deref().unwrap_or(""), &branch, pr_branch)?;
        return Ok(
            json!({"mode":options.mode,"status":"published","published":true,"commit":published_commit,"previous":head,"branch":pr_branch,"pull_request":pull_request,"owned_paths":payload.keys().collect::<Vec<_>>()}),
        );
    }
    // Lock real index while updating only owned entries. Never replace unrelated staging.
    let index =
        PathBuf::from(git.text(&["rev-parse", "--path-format=absolute", "--git-path", "index"])?);
    validation::reject_link(
        index
            .parent()
            .ok_or_else(|| Error::new("path", "Git index has no parent"))?,
    )?;
    if index.exists() {
        validation::reject_link(&index)?;
    }
    let lock = PathBuf::from(format!("{}.lock", index.display()));
    let mut locked = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&lock)
        .map_err(|_| {
            Error::new(
                "publication-conflict",
                "Git index is locked by another operation",
            )
        })?;
    cleanup.lock = Some(lock.clone());
    if index.exists() {
        locked.write_all(&fs::read(&index)?)?;
    }
    locked.sync_all()?;
    drop(locked);
    let next_index = Git {
        root: git.root.clone(),
        index: Some(lock.to_string_lossy().into_owned()),
        options,
    };
    if !index.exists() {
        next_index.run(&["read-tree", &head], None)?;
    }
    for path in path_list(next_index.run(&["diff", "--cached", "--name-only", "-z", &head], None)?)?
    {
        if owned(&path) {
            return Err(Error::new(
                "staged-conflict",
                "Intended-path staging changed before index lock; review again",
            ));
        }
    }
    next_index.run(&["update-index", "-z", "--index-info"], Some(info))?;
    verify_payload(&root, &payload)?;
    verify_head(&git, &branch, &head)?;
    if remote_mode {
        verify_remote(&root, &git, options, &branch)?;
        push(&git, &commit, &branch)?;
    }
    git.run(&["update-ref", &format!("refs/heads/{branch}"), &commit, &head], None)
        .map_err(|_| {
            Error::new(
                "publication-conflict",
                "Local branch advanced; published commit may exist remotely, inspect exact remote before retry",
            )
        })?;
    fs::rename(&lock, &index).map_err(|_| {
        Error::new(
            "publication-recovery",
            "Commit created but index promotion failed; inspect intended staged paths without resetting unrelated work",
        )
    })?;
    cleanup.lock = None;
    Ok(
        json!({"mode":options.mode,"status":"committed","published":remote_mode,"commit":commit,"previous":head,"owned_paths":payload.keys().collect::<Vec<_>>()}),
    )
}
