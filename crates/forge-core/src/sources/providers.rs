use crate::{Error, Result, model::Source, process, validation};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

// Keep extended paths for filesystem validation; normalize only the Git execution boundary.
fn external_path(value: &str) -> String {
    if cfg!(windows) {
        if let Some(path) = value.strip_prefix("\\\\?\\UNC\\") {
            return format!("\\\\{path}");
        }
        if let Some(path) = value.strip_prefix("\\\\?\\") {
            return path.to_owned();
        }
    }
    value.to_owned()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    transport: String,
    host: String,
    reviewed: bool,
}

pub fn validate(source: &Source) -> Result<()> {
    if let Some(key) = &source.credential_reference {
        if key.is_empty()
            || key.len() > 64
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            || !key.as_bytes()[0].is_ascii_alphabetic()
        {
            return Err(Error::new(
                "credential-reference",
                "Credential reference must be a public key name, not a value, URL or path",
            ));
        }
        if source.repository.starts_with("local:") {
            return Err(Error::new(
                "credential-reference",
                "Local sources do not use credentials",
            ));
        }
    }
    repository(&source.repository, source.metadata.get("provider_policy"))
}

pub fn repository(repository: &str, policy: Option<&serde_json::Value>) -> Result<()> {
    if let Some(path) = repository.strip_prefix("local:") {
        if !Path::new(path).is_absolute() || path.contains(['\0', '\r', '\n']) {
            return Err(Error::new(
                "repository",
                "Local sources require an explicit absolute path",
            ));
        }
        if policy.is_some() {
            return Err(Error::new(
                "provider-policy",
                "Local sources do not use transport provider policies",
            ));
        }
        return Ok(());
    }
    let (transport, address) = repository
        .split_once("://")
        .filter(|(t, _)| matches!(*t, "https" | "ssh"))
        .ok_or_else(|| {
            Error::new(
                "repository",
                "Use credential-free HTTPS, reviewed ssh:// or explicit local: source",
            )
        })?;
    if repository.len() > 2048
        || !repository.is_ascii()
        || repository
            .bytes()
            .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
        || repository.contains(['?', '#', '%', '\\'])
    {
        return Err(Error::new(
            "repository",
            "Repository URL contains unsupported or credential-bearing syntax",
        ));
    }
    let (authority, path) = address
        .split_once('/')
        .ok_or_else(|| Error::new("repository", "Repository URL requires a path"))?;
    let host = if transport == "ssh" {
        let (user, host) = authority
            .split_once('@')
            .ok_or_else(|| Error::new("repository", "SSH URL requires an explicit user"))?;
        if user.is_empty()
            || !user
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            || user.starts_with('-')
        {
            return Err(Error::new("repository", "Invalid SSH user"));
        }
        host
    } else {
        authority
    };
    let valid_host = host.len() <= 253
        && host.contains('.')
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        });
    if !valid_host
        || path.is_empty()
        || path.split('/').any(|p| {
            p.is_empty()
                || matches!(p, "." | "..")
                || !p
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'))
        })
    {
        return Err(Error::new(
            "repository",
            "Repository URL requires a canonical DNS host and repository path",
        ));
    }
    match policy {
        Some(value) => {
            let policy: Policy = serde_json::from_value(value.clone())
                .map_err(|_| Error::new("provider-policy", "Invalid reviewed provider policy"))?;
            if !policy.reviewed || policy.host != host || policy.transport != transport {
                return Err(Error::new(
                    "provider-policy",
                    "Provider policy must explicitly review the exact repository host and transport",
                ));
            }
        }
        None if transport == "https"
            && ["github.com", "gitlab.com", "bitbucket.org", "codeberg.org"].contains(&host) => {}
        None => {
            return Err(Error::new(
                "host-review",
                "Repository requires an explicit reviewed provider policy",
            ));
        }
    }
    Ok(())
}

pub fn acquisition(source: &Source, offline: bool) -> Result<()> {
    validate(source)?;
    if !offline && source.credential_reference.is_some() {
        return Err(Error::new(
            "credential-provider-unavailable",
            "Authenticated acquisition is blocked: no approved store/environment process boundary is integrated; credential reference was not resolved",
        ));
    }
    if !offline && source.repository.starts_with("ssh://") {
        return Err(Error::new(
            "ssh-provider-unavailable",
            "SSH acquisition is blocked: approved agent identity and pinned host-key isolation are unavailable; no user SSH credentials or config will be read",
        ));
    }
    Ok(())
}

// The shared process owner is reused; this context carries only public isolation settings.
pub struct Transport {
    home: Option<PathBuf>,
}
impl Drop for Transport {
    fn drop(&mut self) {
        if let Some(home) = &self.home {
            let _ = fs::remove_dir_all(home);
        }
    }
}
impl Transport {
    pub fn new(state: &Path, source: &Source) -> Result<Self> {
        if source.repository.starts_with("local:") {
            return Ok(Self { home: None });
        }
        // git_env cannot remove inherited variables. Refuse overrides before spawning Git.
        for (name, _) in std::env::vars_os() {
            let key = name.to_string_lossy().to_ascii_uppercase();
            if (key.starts_with("GIT_")
                && !matches!(
                    key.as_str(),
                    "GIT_TERMINAL_PROMPT" | "GIT_CONFIG_NOSYSTEM" | "GIT_CONFIG_GLOBAL"
                ))
                || matches!(
                    key.as_str(),
                    "HTTP_PROXY" | "HTTPS_PROXY" | "ALL_PROXY" | "CURL_HOME" | "SSLKEYLOGFILE"
                )
            {
                return Err(Error::new(
                    "provider-environment",
                    "Inherited Git/HTTP override prevents isolated acquisition; launch with a clean transport environment",
                ));
            }
        }
        let home = state.join(format!(
            "provider-home-{}",
            crate::transaction::operation_id()
        ));
        fs::create_dir(&home)?;
        Ok(Self { home: Some(home) })
    }
    pub fn git(&self, args: &[String], timeout: u64) -> Result<Vec<u8>> {
        let Some(home) = &self.home else {
            return process::git(args, timeout);
        };
        let directory = external_path(&home.to_string_lossy());
        let ceiling = external_path(
            &home
                .parent()
                .ok_or_else(|| Error::new("path", "Missing provider state"))?
                .to_string_lossy(),
        );
        let mut command = vec![
            "-C".into(),
            directory.to_string(),
            "--no-replace-objects".into(),
            "--no-lazy-fetch".into(),
        ];
        for setting in [
            "credential.helper=",
            "core.askPass=",
            "http.extraHeader=",
            "http.sslVerify=true",
            "http.proxy=",
            "http.cookieFile=",
            "http.saveCookies=false",
            "http.followRedirects=false",
            "fetch.recurseSubmodules=false",
            "maintenance.auto=false",
            "gc.auto=0",
            "init.templateDir=",
        ] {
            command.extend(["-c".into(), setting.into()]);
        }
        command.extend(args.iter().enumerate().map(|(index, value)| {
            if index > 0 && matches!(args[index - 1].as_str(), "-C" | "--git-dir" | "--file") {
                external_path(value)
            } else {
                value.clone()
            }
        }));
        process::git_env(
            &command,
            timeout,
            &[
                ("HOME", &directory),
                ("USERPROFILE", &directory),
                ("XDG_CONFIG_HOME", &directory),
                ("GIT_CEILING_DIRECTORIES", &ceiling),
                ("GIT_ALLOW_PROTOCOL", "https"),
                ("GIT_ASKPASS", ""),
                ("SSH_ASKPASS", ""),
                ("GIT_TRACE_REDACT", "1"),
            ],
        )
    }
    pub fn verify_cache(&self, bare: &Path, repository: &str) -> Result<()> {
        validation::reject_link(bare)?;
        let config = bare.join("config");
        validation::reject_link(&config)?;
        for path in [
            "objects/info/alternates",
            "objects/info/http-alternates",
            "commondir",
            "config.worktree",
        ] {
            if bare.join(path).exists() {
                return Err(Error::new(
                    "cache-policy",
                    "Cached repository has unsupported external configuration or object stores; use a new source ID",
                ));
            }
        }
        let bytes = self.git(
            &[
                "config".into(),
                "--no-includes".into(),
                "--file".into(),
                config.to_string_lossy().into_owned(),
                "--null".into(),
                "--list".into(),
            ],
            30,
        )?;
        let listing = std::str::from_utf8(&bytes)
            .map_err(|_| Error::new("cache-policy", "Cached configuration is not UTF-8"))?;
        for entry in listing.split('\0').filter(|e| !e.is_empty()) {
            let (key, value) = entry
                .split_once('\n')
                .ok_or_else(|| Error::new("cache-policy", "Invalid cached configuration"))?;
            let allowed = match key {
                "core.repositoryformatversion" => value == "0",
                "core.bare" => value == "true",
                "core.filemode"
                | "core.ignorecase"
                | "core.symlinks"
                | "core.logallrefupdates"
                | "core.precomposeunicode" => {
                    matches!(value, "true" | "false")
                }
                "remote.origin.url" => value == repository,
                "remote.origin.fetch" => value == "+refs/heads/*:refs/heads/*",
                "remote.origin.tagopt" => value == "--no-tags",
                _ => false,
            };
            if !allowed {
                return Err(Error::new(
                    "cache-policy",
                    "Cached repository contains unreviewed configuration; retain accepted state and use a new source ID",
                ));
            }
        }
        Ok(())
    }
}
