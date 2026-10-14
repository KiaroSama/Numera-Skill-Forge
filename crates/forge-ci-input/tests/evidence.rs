use std::{fs, path::PathBuf, process::Command};

#[test]
fn changed_or_extra_projection_input_is_refused() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.ci-work/input-drift")
        .join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir_all(root.join("input")).unwrap();
    fs::write(root.join("input/Cargo.toml"), "expected").unwrap();
    let manifest = serde_json::json!({"schema":1,"contract":"forge-linux-input-v1","source_revision":"a".repeat(40),"files":[{"path":"Cargo.toml","mode":"100644","bytes":8,"sha256":"cea23dd4b87e8b00d19fb9ccaaef93e97353c7353e2070f3baf05aeb3995dff4"}]});
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let valid = Command::new(env!("CARGO_BIN_EXE_forge-ci-input"))
        .args(["verify", "--projection"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        valid.status.success(),
        "Valid declared projection must pass before drift"
    );
    fs::write(root.join("input/Cargo.toml"), "tampered").unwrap();
    let changed = Command::new(env!("CARGO_BIN_EXE_forge-ci-input"))
        .args(["verify", "--projection"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(!changed.status.success());
    fs::write(root.join("input/Cargo.toml"), "expected").unwrap();
    fs::write(root.join("input/ambient.txt"), "undeclared").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_forge-ci-input"))
        .args(["verify", "--projection"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    fs::remove_dir_all(root).unwrap();
}
