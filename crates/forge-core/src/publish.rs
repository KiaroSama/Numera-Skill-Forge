use crate::{Error, Result, process, storage, validation};
use serde_json::{Value, json};
use std::{fs, path::Path};

pub fn publish(
    root: &Path,
    mode: &str,
    expected_remote: Option<&str>,
    author_email: Option<&str>,
) -> Result<Value> {
    let root_text = root.to_string_lossy().into_owned();
    let remote = process::text(&["-C", &root_text, "remote", "get-url", "origin"])?;
    let branch = process::text(&["-C", &root_text, "branch", "--show-current"])?;
    let mut paths = vec![];
    for prefix in ["registry", "packages", "catalogs"] {
        let directory = root.join(prefix);
        if directory.exists() {
            for (path, _) in storage::files(&directory)? {
                paths.push(format!("{prefix}/{path}"));
            }
        }
    }
    if root.join("forge.lock.json").exists() {
        paths.push("forge.lock.json".into());
    }
    paths.sort();
    if mode == "preview" || mode == "local" {
        return Ok(
            json!({"mode":mode,"remote":remote.trim(),"branch":branch.trim(),"owned_paths":paths,"published":false}),
        );
    }
    if !["commit", "direct-push", "pull-request"].contains(&mode) {
        return Err(Error::new(
            "publish-mode",
            "Expected preview/local/commit/pull-request/direct-push",
        ));
    }
    let expected = expected_remote.ok_or_else(|| {
        Error::new(
            "publication-review",
            "Explicit expected_remote is required before Git mutation",
        )
    })?;
    if remote.trim() != expected {
        return Err(Error::new(
            "remote-mismatch",
            "Actual origin differs from configured publication repository",
        ));
    }
    let email = author_email.ok_or_else(|| {
        Error::new(
            "author-required",
            "Configure an explicit public author email",
        )
    })?;
    if email.contains(['\r', '\n', '\0']) || !email.contains('@') {
        return Err(Error::new("author", "Invalid author email"));
    }
    let status = process::text(&["-C", &root_text, "status", "--porcelain"])?;
    if status
        .lines()
        .any(|line| line.len() > 3 && !paths.iter().any(|p| line[3..].trim_matches('"') == *p))
    {
        return Err(Error::new(
            "dirty-worktree",
            "Publication requires an isolated checkout; unrelated changes are preserved",
        ));
    }
    if mode == "pull-request" {
        return Err(Error::new(
            "pr-setup-required",
            "Configure and review a dedicated publication branch and authenticated GitHub client first",
        ));
    }
    let head = process::text(&["-C", &root_text, "rev-parse", "HEAD"])?;
    let index = root.join(".forge-state/publish.index");
    if index.exists() {
        return Err(Error::new(
            "publication-conflict",
            "Prior publication index requires inspection",
        ));
    }
    let outcome = (|| -> Result<Value> {
        let git_index = index.to_string_lossy().into_owned();
        let run = |tail: Vec<String>| -> Result<Vec<u8>> {
            let mut a = vec!["-C".into(), root_text.clone()];
            a.extend(tail);
            process::git_env(
                &a,
                60,
                &[
                    ("GIT_INDEX_FILE", &git_index),
                    ("GIT_AUTHOR_EMAIL", email),
                    ("GIT_COMMITTER_EMAIL", email),
                ],
            )
        };
        run(vec!["read-tree".into(), "HEAD".into()])?;
        if !process::text(&["-C", &root_text, "diff", "--cached", "--name-only"])?
            .trim()
            .is_empty()
        {
            return Err(Error::new(
                "staged-conflict",
                "Existing staged changes are not owned by publication",
            ));
        }
        for p in &paths {
            validation::relative_path(p)?;
        }
        let mut add = vec!["add".into(), "--".into()];
        add.extend(paths.clone());
        run(add)?;
        let changes = String::from_utf8(run(vec![
            "diff".into(),
            "--cached".into(),
            "--name-only".into(),
        ])?)
        .map_err(|_| Error::new("encoding", "Publication paths are not UTF-8"))?;
        if changes.trim().is_empty() {
            return Ok(json!({"mode":mode,"status":"unchanged","published":false}));
        }
        run(vec![
            "commit".into(),
            "-m".into(),
            "chore: update packages".into(),
        ])?;
        if mode == "direct-push" {
            run(vec!["push".into(), "origin".into(), branch.trim().into()])?;
        }
        let commit = process::text(&["-C", &root_text, "rev-parse", "HEAD"])?;
        Ok(
            json!({"mode":mode,"commit":commit.trim(),"previous":head.trim(),"published":mode=="direct-push"}),
        )
    })();
    if index.exists() {
        let _ = fs::remove_file(index);
    }
    outcome
}
