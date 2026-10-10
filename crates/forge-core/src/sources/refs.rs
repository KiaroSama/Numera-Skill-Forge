use crate::{
    Error, Result,
    model::{RefPolicy, Source},
    process,
};
use std::path::Path;

pub fn resolve(source: &Source, _origin: &Path, offline: bool) -> Result<String> {
    match &source.reference {
        RefPolicy::DefaultBranch => {
            if source.repository.starts_with("local:") || offline {
                return Ok("HEAD".into());
            }
            let args = vec![
                "ls-remote".into(),
                "--symref".into(),
                source.repository.clone(),
                "HEAD".into(),
            ];
            let result = String::from_utf8(process::git(&args, 30)?)
                .map_err(|_| Error::new("encoding", "Invalid default branch response"))?;
            let branch = result
                .lines()
                .find_map(|line| {
                    line.strip_prefix("ref: ")
                        .and_then(|v| v.split_once('\t'))
                        .filter(|(_, head)| *head == "HEAD")
                        .map(|(reference, _)| reference.to_owned())
                })
                .ok_or_else(|| Error::new("default-branch", "Remote has no default branch"))?;
            if !branch.starts_with("refs/heads/") {
                return Err(Error::new("default-branch", "Remote HEAD is not a branch"));
            }
            Ok(branch)
        }
        RefPolicy::Branch(name) => Ok(format!("refs/heads/{name}")),
        RefPolicy::Commit(sha) => {
            if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(Error::new(
                    "revision",
                    "Pinned commit must be an immutable SHA",
                ));
            }
            Ok(sha.clone())
        }
        RefPolicy::Tag(name) => Ok(format!("refs/tags/{name}")),
        RefPolicy::LatestStableRelease => {
            if offline {
                return Err(Error::new(
                    "offline-release",
                    "Latest release requires an online provider; pin a release tag for offline use",
                ));
            }
            let repo = source
                .repository
                .strip_prefix("https://github.com/")
                .ok_or_else(|| {
                    Error::new(
                        "release-provider",
                        "Latest stable release provider supports GitHub HTTPS sources",
                    )
                })?
                .trim_end_matches(".git");
            if repo.split('/').count() != 2
                || !repo
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
            {
                return Err(Error::new("repository", "Invalid GitHub release identity"));
            }
            // The bounded provider shares Git's pipe draining, cancellation and process ownership.
            let output = process::github(
                &[
                    "api".into(),
                    "--hostname".into(),
                    "github.com".into(),
                    format!("repos/{repo}/releases/latest"),
                    "--jq".into(),
                    ".tag_name".into(),
                ],
                30,
            )?;
            let tag = String::from_utf8(output)
                .map_err(|_| Error::new("encoding", "Release tag is not UTF-8"))?
                .trim()
                .to_owned();
            if tag.is_empty() || tag.starts_with('-') || tag.contains(['\r', '\n', '\0']) {
                return Err(Error::new("ref", "Invalid published release tag"));
            }
            Ok(format!("refs/tags/{tag}"))
        }
    }
}
