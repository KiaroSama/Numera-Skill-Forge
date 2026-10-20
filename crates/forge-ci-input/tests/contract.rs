use std::{fs, path::PathBuf, process::Command};

#[test]
fn projection_uses_committed_bytes_and_omits_ambient_inputs() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.ci-work/input-contract-test")
        .join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir_all(&root).unwrap();
    let repo = root.join("repo");
    fs::create_dir(&repo).unwrap();
    let git = |args: &[&str]| {
        let result = Command::new("git")
            .current_dir(&repo)
            .args([
                "-c",
                "core.hooksPath=",
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(result.status.success(), "Git fixture failed");
        result.stdout
    };
    git(&["init", "-q"]);
    fs::write(repo.join("Cargo.toml"), "# committed α\n").unwrap();
    fs::write(repo.join("LICENSE"), "Committed legal fixture\n").unwrap();
    git(&["add", "Cargo.toml", "LICENSE"]);
    git(&["commit", "-qm", "fixture"]);
    let revision = String::from_utf8(git(&["rev-parse", "HEAD"])).unwrap();
    fs::write(repo.join("Cargo.toml"), "# ambient change\n").unwrap();
    fs::write(repo.join("secrets.md"), "inert private sentinel").unwrap();
    let output = root.join("projection");
    let result = Command::new(env!("CARGO_BIN_EXE_forge-ci-input"))
        .args(["project", "--repository"])
        .arg(&repo)
        .args(["--revision", revision.trim(), "--output"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(output.join("input/Cargo.toml")).unwrap(),
        "# committed α\n".as_bytes()
    );
    assert!(!output.join("input/secrets.md").exists());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["source_revision"], revision.trim());
    assert_eq!(manifest["files"].as_array().unwrap().len(), 2);
    assert_eq!(
        fs::read(output.join("input/LICENSE")).unwrap(),
        b"Committed legal fixture\n"
    );
    // A selected symlink Git mode is refused even without creating an OS symlink on Windows.
    git(&[
        "update-index",
        "--cacheinfo",
        "120000",
        String::from_utf8(git(&["rev-parse", "HEAD:Cargo.toml"]))
            .unwrap()
            .trim(),
        "Cargo.toml",
    ]);
    git(&["commit", "-qm", "unsafe mode"]);
    let unsafe_revision = String::from_utf8(git(&["rev-parse", "HEAD"])).unwrap();
    let refused = Command::new(env!("CARGO_BIN_EXE_forge-ci-input"))
        .args(["project", "--repository"])
        .arg(&repo)
        .args(["--revision", unsafe_revision.trim(), "--output"])
        .arg(root.join("unsafe"))
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(refused.stdout.is_empty());
    fs::remove_dir_all(root).unwrap();
}
