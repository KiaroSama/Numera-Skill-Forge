use forge_core::{process, publish::PublishOptions, storage};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let mut command = vec!["-C".into(), root.to_string_lossy().into_owned()];
    command.extend(args.iter().map(|v| v.to_string()));
    process::git_env(
        &command,
        15,
        &[
            ("GIT_AUTHOR_NAME", "Fixture"),
            ("GIT_COMMITTER_NAME", "Fixture"),
            ("GIT_AUTHOR_EMAIL", "Kiaro.Sama.Dev@gmail.com"),
            ("GIT_COMMITTER_EMAIL", "Kiaro.Sama.Dev@gmail.com"),
        ],
    )
    .unwrap()
}
fn publish_request(root: &Path, options: &PublishOptions) -> forge_core::Result<serde_json::Value> {
    let mut request = serde_json::to_value(options)?;
    request["action"] = serde_json::json!("publish");
    forge_core::service::execute(root, request)
}
fn text(root: &Path, args: &[&str]) -> String {
    String::from_utf8(git(root, args))
        .unwrap()
        .trim()
        .to_owned()
}

// One owned, non-network seam fixture; no sleeps, Git children bounded by the core runner.
#[test]
fn intended_publication_preserves_bytes_modes_index_and_remote_guards() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.ci-work/tests")
        .join(format!("publication-{}", std::process::id()));
    assert!(!root.exists());
    fs::create_dir_all(&root).unwrap();
    let _fixture = Fixture(root.clone());
    git(&root, &["init", "-b", "main"]);
    fs::write(root.join(".gitignore"), b"/.forge-state/\n/remote.git/\n").unwrap();
    fs::write(root.join("unrelated.txt"), b"original user bytes\n").unwrap();
    fs::create_dir_all(root.join("packages/demo")).unwrap();
    fs::write(root.join("packages/demo/run.sh"), b"first\r\n").unwrap();
    git(&root, &["add", ".gitignore", "unrelated.txt", "packages"]);
    git(&root, &["commit", "-m", "fixture"]);
    let remote = root.join("remote.git");
    git(&root, &["init", "--bare", remote.to_str().unwrap()]);
    git(
        &root,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(
        &root,
        &["-c", "protocol.file.allow=always", "push", "origin", "main"],
    );
    let initial = text(&root, &["rev-parse", "HEAD"]);
    let mut options = PublishOptions {
        mode: "direct-push".into(),
        expected_remote: Some(remote.to_string_lossy().into_owned()),
        expected_head: Some(initial.clone()),
        expected_remote_head: Some(initial.clone()),
        author_email: Some("Kiaro.Sama.Dev@gmail.com".into()),
        author_name: Some("Fixture".into()),
        allow_owned_local_remote: true,
        ..Default::default()
    };
    // A clean no-op does not create a commit or publish.
    let noop = publish_request(&root, &options).unwrap();
    assert_eq!(noop["status"], "unchanged");
    assert_eq!(text(&root, &["rev-parse", "HEAD"]), initial);
    fs::write(root.join("unrelated.txt"), b"staged user edit\n").unwrap();
    git(&root, &["add", "unrelated.txt"]);
    fs::write(root.join("unrelated.txt"), b"unstaged user edit\n").unwrap();
    let staged = git(&root, &["show", ":unrelated.txt"]);
    // This filter must never execute; publication hashes raw bytes, not Git add filters.
    git(
        &root,
        &["config", "filter.unsafe.clean", "nonexistent-numera-filter"],
    );
    fs::write(
        root.join(".gitattributes"),
        b"packages/** filter=unsafe text eol=lf\n",
    )
    .unwrap();
    let payload = b"#!/bin/sh\r\n# UTF-8: caf\xc3\xa9\r\n";
    fs::write(root.join("packages/demo/run.sh"), payload).unwrap();
    storage::write(
        &root.join("packages/demo/provenance.json"),
        &serde_json::json!({"file_modes":{"run.sh":493}}),
    )
    .unwrap();
    options.expected_remote_head = Some("0000000000000000000000000000000000000000".into());
    assert_eq!(
        publish_request(&root, &options).unwrap_err().code,
        "remote-advanced"
    );
    assert_eq!(text(&root, &["rev-parse", "HEAD"]), initial);
    options.expected_remote_head = Some(initial.clone());
    options.expected_remote = Some("https://github.com/example/wrong.git".into());
    assert_eq!(
        publish_request(&root, &options).unwrap_err().code,
        "remote-mismatch"
    );
    options.expected_remote = Some(remote.to_string_lossy().into_owned());
    let result = publish_request(&root, &options).unwrap();
    assert_eq!(result["published"], true);
    let commit = result["commit"].as_str().unwrap();
    assert_eq!(text(&root, &["rev-parse", "HEAD"]), commit);
    assert_eq!(
        text(
            &root,
            &[
                "--git-dir",
                remote.to_str().unwrap(),
                "rev-parse",
                "refs/heads/main"
            ]
        ),
        commit
    );
    assert_eq!(
        git(&root, &["show", &format!("{commit}:packages/demo/run.sh")]),
        payload
    );
    assert_eq!(
        git(&root, &["show", &format!("{commit}:unrelated.txt")]),
        b"original user bytes\n"
    );
    assert_eq!(git(&root, &["show", ":unrelated.txt"]), staged);
    assert_eq!(
        fs::read(root.join("unrelated.txt")).unwrap(),
        b"unstaged user edit\n"
    );
    assert!(text(&root, &["ls-tree", commit, "packages/demo/run.sh"]).starts_with("100755"));
    assert_eq!(git(&root, &["show", ":packages/demo/run.sh"]), payload);
    // Stale local HEAD refuses before further mutation; deletion and no-op use exact guards.
    assert_eq!(
        publish_request(&root, &options).unwrap_err().code,
        "head-advanced"
    );
    options.expected_head = Some(commit.into());
    options.expected_remote_head = Some(commit.into());
    fs::remove_file(root.join("packages/demo/run.sh")).unwrap();
    fs::remove_file(root.join("packages/demo/provenance.json")).unwrap();
    let deleted = publish_request(&root, &options).unwrap();
    let deletion = deleted["commit"].as_str().unwrap();
    assert!(text(&root, &["ls-tree", deletion, "packages/demo/run.sh"]).is_empty());
    assert_eq!(git(&root, &["show", ":unrelated.txt"]), staged);
    options.expected_head = Some(deletion.into());
    options.expected_remote_head = Some(deletion.into());
    assert_eq!(
        publish_request(&root, &options).unwrap()["status"],
        "unchanged"
    );
    fs::write(root.join("packages/demo/secrets.md"), b"excluded marker").unwrap();
    assert_eq!(
        publish_request(&root, &options).unwrap_err().code,
        "protected-publication"
    );
    assert_eq!(text(&root, &["rev-parse", "HEAD"]), deletion);
    assert!(!root.join(".git/index.lock").exists());
}
