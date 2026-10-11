use forge_core::{service, storage, transaction};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(transaction::operation_id());
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/state-compatibility/first-release.json"
    ))
    .unwrap()
}

#[test]
fn first_release_registry_lock_and_receipt_keep_identity_through_current_lifecycle() {
    let workspace = Workspace::new();
    let mut old = fixture();
    let destination = workspace.0.join("first-release project سلام");
    fs::create_dir(&destination).unwrap();
    old["receipt"]["destination"] = json!(destination);
    let registry = workspace.0.join("registry/sources.json");
    let lock = workspace.0.join("forge.lock.json");
    let receipt = workspace
        .0
        .join(".forge-state/receipts/legacy-claude-code-first-release.json");
    storage::write(&registry, &old["registry"]).unwrap();
    storage::write(&lock, &old["lock"]).unwrap();
    storage::write(&receipt, &old["receipt"]).unwrap();
    let original_receipt = fs::read(&receipt).unwrap();
    let payload = b"First-release owned skill.\r\n";
    let installed = destination.join(".claude/skills/legacy/SKILL.md");
    let packaged = workspace
        .0
        .join("packages/claude-code/legacy/skills/legacy/SKILL.md");
    storage::atomic(&installed, payload).unwrap();
    storage::atomic(&packaged, payload).unwrap();

    let status = service::execute(&workspace.0, json!({"action":"status"})).unwrap();
    assert_eq!(status["sources"], 1);
    assert_eq!(status["packages"], 1);
    assert_eq!(status["installed"], 1);
    let loaded = service::execute(&workspace.0, json!({"action":"installed"})).unwrap();
    assert_eq!(loaded[0]["id"], "legacy-claude-code-first-release");
    assert_eq!(loaded[0]["scope"], "project");
    assert_eq!(loaded[0]["modes"], json!({}));
    assert_eq!(fs::read(&receipt).unwrap(), original_receipt);

    service::execute(&workspace.0, json!({"action":"init"})).unwrap();
    assert_eq!(storage::read::<Value>(&registry).unwrap(), old["registry"]);
    assert_eq!(storage::read::<Value>(&lock).unwrap(), old["lock"]);
    let updated = service::execute(
        &workspace.0,
        json!({"action":"install","package":"legacy","target":"claude-code","destination":destination,"dry_run":false}),
    )
    .unwrap();
    assert_eq!(updated["id"], "legacy-claude-code-first-release");
    assert_eq!(updated["files"], old["receipt"]["files"]);
    assert_eq!(fs::read(&installed).unwrap(), payload);
    assert_eq!(
        service::execute(&workspace.0, json!({"action":"installed"}))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let id = updated["id"].as_str().unwrap();
    service::execute(&workspace.0, json!({"action":"disable","receipt":id})).unwrap();
    assert!(!installed.exists());
    service::execute(&workspace.0, json!({"action":"enable","receipt":id})).unwrap();
    assert_eq!(fs::read(&installed).unwrap(), payload);
    fs::write(&installed, b"User edit after upgrading Forge").unwrap();
    assert_eq!(
        service::execute(&workspace.0, json!({"action":"uninstall","receipt":id}))
            .unwrap_err()
            .code,
        "conflict"
    );
    assert_eq!(
        fs::read(&installed).unwrap(),
        b"User edit after upgrading Forge"
    );
    fs::write(&installed, payload).unwrap();
    service::execute(&workspace.0, json!({"action":"uninstall","receipt":id})).unwrap();
    assert!(!installed.exists());
    assert!(!receipt.exists());
    assert_eq!(storage::read::<Value>(&registry).unwrap(), old["registry"]);
    assert_eq!(storage::read::<Value>(&lock).unwrap(), old["lock"]);
}

#[test]
fn first_job_format_retries_without_progress_and_preserves_previous_record() {
    let workspace = Workspace::new();
    let old = fixture();
    let path = workspace.0.join(".forge-state/jobs/first-release-job.json");
    storage::write(&path, &old["job"]).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert_eq!(
        service::execute(
            &workspace.0,
            json!({"action":"job-status","job":"first-release-job"})
        )
        .unwrap(),
        old["job"]
    );
    let retry = service::execute(
        &workspace.0,
        json!({"action":"job-retry","job":"first-release-job"}),
    )
    .unwrap();
    assert_ne!(retry["id"], old["job"]["id"]);
    assert_eq!(retry["retry_of"], "first-release-job");
    assert_eq!(retry["request"], old["job"]["request"]);
    assert_eq!(retry["timeout"], 30);
    assert_eq!(retry["status"], "failed");
    assert_eq!(retry["error"]["code"], "source");
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn unsupported_state_versions_fail_without_rewriting_the_selected_file() {
    for (name, key) in [
        ("registry/sources.json", "registry"),
        ("forge.lock.json", "lock"),
    ] {
        let workspace = Workspace::new();
        let mut state = fixture()[key].clone();
        state["schema_version"] = json!(99);
        let path = workspace.0.join(name);
        storage::write(&path, &state).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(
            service::execute(&workspace.0, json!({"action":"init"}))
                .unwrap_err()
                .code,
            "schema-version"
        );
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}
