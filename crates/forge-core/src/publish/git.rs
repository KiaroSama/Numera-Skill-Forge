use super::PublishOptions;
use crate::{Error, Result, process};

// Windows canonical paths remain extended for containment checks, but Git needs ordinary paths.
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
pub(super) struct Git<'a> {
    pub(super) root: String,
    pub(super) index: Option<String>,
    pub(super) options: &'a PublishOptions,
}
impl Git<'_> {
    pub(super) fn run(&self, args: &[&str], input: Option<Vec<u8>>) -> Result<Vec<u8>> {
        let mut command = vec![
            "-C".into(),
            external_path(&self.root),
            "-c".into(),
            "core.fsmonitor=false".into(),
        ];
        if self.options.allow_owned_local_remote {
            command.extend(["-c".into(), "protocol.file.allow=always".into()]);
        }
        command.extend(args.iter().enumerate().map(|(i, value)| {
            if i > 0 && matches!(args[i - 1], "--git-dir" | "--work-tree" | "-C") {
                external_path(value)
            } else {
                (*value).to_owned()
            }
        }));
        let index = self.index.as_deref().map(external_path);
        let mut env = vec![("GIT_NO_REPLACE_OBJECTS", "1")];
        if let Some(index) = index.as_deref() {
            env.push(("GIT_INDEX_FILE", index));
        }
        if let Some(name) = self.options.author_name.as_deref() {
            env.extend([("GIT_AUTHOR_NAME", name), ("GIT_COMMITTER_NAME", name)]);
        }
        if let Some(email) = self.options.author_email.as_deref() {
            env.extend([("GIT_AUTHOR_EMAIL", email), ("GIT_COMMITTER_EMAIL", email)]);
        }
        process::git_input(&command, 60, &env, input).map_err(|error| {
            let operation = args
                .iter()
                .find(|arg| {
                    matches!(
                        **arg,
                        "rev-parse"
                            | "remote"
                            | "branch"
                            | "ls-tree"
                            | "ls-remote"
                            | "read-tree"
                            | "write-tree"
                            | "update-index"
                            | "hash-object"
                            | "commit-tree"
                            | "cat-file"
                            | "update-ref"
                            | "push"
                            | "diff"
                            | "check-ref-format"
                    )
                })
                .copied()
                .unwrap_or("git");
            Error::new(
                error.code,
                format!("Publication {operation} failed: {}", error.message),
            )
        })
    }
    pub(super) fn text(&self, args: &[&str]) -> Result<String> {
        String::from_utf8(self.run(args, None)?)
            .map(|s| s.trim().to_owned())
            .map_err(|_| Error::new("encoding", "Git metadata is not UTF-8"))
    }
}

impl Git<'_> {
    pub(super) fn text_input(&self, args: &[&str], input: Vec<u8>) -> Result<String> {
        String::from_utf8(self.run(args, Some(input))?)
            .map(|s| s.trim().into())
            .map_err(|_| Error::new("encoding", "Git object identity is not UTF-8"))
    }
}
