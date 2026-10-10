use super::{Git, PublishOptions, object_id, required};
use crate::{Error, Result, process, validation};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn github_repository(remote: &str) -> Result<String> {
    let path = remote
        .strip_prefix("https://github.com/")
        .or_else(|| remote.strip_prefix("git@github.com:"))
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))
        .ok_or_else(|| {
            Error::new(
                "publication-remote",
                "Use an explicit credential-free github.com repository URL",
            )
        })?
        .trim_end_matches(".git");
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|p| {
            p.is_empty()
                || !p
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                || *p == "."
                || *p == ".."
        })
    {
        return Err(Error::new(
            "publication-remote",
            "Invalid GitHub repository identity",
        ));
    }
    Ok(path.into())
}
pub(super) fn verify_remote(
    root: &Path,
    git: &Git<'_>,
    options: &PublishOptions,
    branch: &str,
) -> Result<Option<String>> {
    let expected = required(&options.expected_remote, "expected_remote")?;
    if git.text(&["remote", "get-url", "origin"])? != expected
        || git.text(&["remote", "get-url", "--push", "origin"])? != expected
    {
        return Err(Error::new(
            "remote-mismatch",
            "Fetch/push origin differs from reviewed publication repository",
        ));
    }
    let repository = if options.allow_owned_local_remote {
        if expected.starts_with("file:") {
            return Err(Error::new(
                "publication-remote",
                "Local testing accepts an owned bare directory, not file URLs",
            ));
        }
        let physical = PathBuf::from(expected);
        let physical = if physical.is_absolute() {
            physical
        } else {
            root.join(physical)
        };
        let canonical = fs::canonicalize(&physical)?;
        let relative = canonical.strip_prefix(root).map_err(|_| {
            Error::new(
                "publication-remote",
                "Local remote must be physically inside the owned workspace",
            )
        })?;
        if relative.as_os_str().is_empty() {
            return Err(Error::new(
                "publication-remote",
                "Workspace cannot be its own publication remote",
            ));
        }
        validation::contained(root, &relative.to_string_lossy().replace('\\', "/"))?;
        let path = canonical.to_string_lossy();
        if git.text(&["--git-dir", &path, "rev-parse", "--is-bare-repository"])? != "true" {
            return Err(Error::new(
                "publication-remote",
                "Owned local test remote must be bare",
            ));
        }
        None
    } else {
        let repository = github_repository(expected)?;
        let account = required(&options.expected_account, "expected_account")?;
        let active = String::from_utf8(process::github(
            &[
                "api".into(),
                "--hostname".into(),
                "github.com".into(),
                "user".into(),
                "--jq".into(),
                ".login".into(),
            ],
            30,
        )?)
        .map_err(|_| Error::new("encoding", "GitHub identity is not UTF-8"))?;
        if active.trim() != account {
            return Err(Error::new(
                "account-mismatch",
                "Active GitHub account differs from reviewed publication account",
            ));
        }
        Some(repository)
    };
    let expected_head = required(&options.expected_remote_head, "expected_remote_head")?;
    object_id(expected_head)?;
    let output = git.text(&[
        "ls-remote",
        "--refs",
        "origin",
        &format!("refs/heads/{branch}"),
    ])?;
    if output.split_whitespace().next() != Some(expected_head) {
        return Err(Error::new(
            "remote-advanced",
            "Remote base advanced or is missing; review again without force pushing",
        ));
    }
    Ok(repository)
}

pub(super) fn push(git: &Git<'_>, commit: &str, branch: &str) -> Result<()> {
    // A push URL/refspec is explicit; never --force, tags, mirror, or configured broad refspecs.
    git.run(
        &[
            "-c",
            "push.followTags=false",
            "push",
            "--porcelain",
            "origin",
            &format!("{commit}:refs/heads/{branch}"),
        ],
        None,
    )?;
    let actual = git.text(&[
        "ls-remote",
        "--refs",
        "origin",
        &format!("refs/heads/{branch}"),
    ])?;
    if actual.split_whitespace().next() != Some(commit) {
        return Err(Error::new(
            "publication-verification",
            "Remote commit does not match intended published commit",
        ));
    }
    Ok(())
}
pub(super) fn pr(repository: &str, base: &str, branch: &str) -> Result<Value> {
    let query = vec![
        "pr".into(),
        "list".into(),
        "--repo".into(),
        repository.into(),
        "--base".into(),
        base.into(),
        "--head".into(),
        branch.into(),
        "--state".into(),
        "open".into(),
        "--json".into(),
        "number,url,headRefName,baseRefName".into(),
    ];
    let entries: Vec<Value> = serde_json::from_slice(&process::github(&query, 30)?)?;
    if entries.len() > 1 {
        return Err(Error::new(
            "publication-conflict",
            "Multiple PRs exist for intended publication branch",
        ));
    }
    if let Some(entry) = entries.into_iter().next() {
        return Ok(entry);
    }
    process::github(
        &[
            "pr".into(),
            "create".into(),
            "--repo".into(),
            repository.into(),
            "--base".into(),
            base.into(),
            "--head".into(),
            branch.into(),
            "--title".into(),
            "chore: update packages".into(),
            "--body".into(),
            "Update reviewed generated packages.\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)".into(),
        ],
        60,
    )?;
    let entries: Vec<Value> = serde_json::from_slice(&process::github(&query, 30)?)?;
    if entries.len() != 1 {
        return Err(Error::new(
            "publication-verification",
            "Created PR could not be verified by exact repository/base/head identity",
        ));
    }
    Ok(entries[0].clone())
}
