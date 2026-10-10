use forge_core::{
    model::{Payload, Snapshot},
    service, storage, transaction,
};
use serde_json::json;
use std::{fs, path::PathBuf};
#[test]
fn non_posix_runtime_companion_requires_explicit_trust_without_changing_git_mode() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.ci-work/tests")
        .join(transaction::operation_id());
    fs::create_dir_all(&root).unwrap();
    let files = vec![Payload {
        path: "skills/demo/scripts/run.ps1".into(),
        digest: storage::digest(b"never run"),
        size: 9,
        executable: false,
    }];
    let digest = storage::digest(&storage::encoded(&files).unwrap());
    let snapshot = Snapshot {
        source: "demo".into(),
        revision: "a".repeat(40),
        content_digest: digest.clone(),
        license_digest: "test".into(),
        files,
        components: vec![],
    };
    storage::write(&root.join(".forge-state/candidates/demo.json"), &snapshot).unwrap();
    assert_eq!(
        service::execute(
            &root,
            json!({"action":"approve","source":"demo","digest":digest})
        )
        .unwrap_err()
        .code,
        "trust-required"
    );
    service::execute(
        &root,
        json!({"action":"approve","source":"demo","digest":digest,"allow_executable":true}),
    )
    .unwrap();
    let lock = forge_core::registry::lock(&root).unwrap();
    assert!(!lock.sources["demo"].snapshot.files[0].executable);
    assert!(lock.sources["demo"].executable_approved);
    fs::remove_dir_all(root).unwrap();
}
