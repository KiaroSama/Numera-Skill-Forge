use forge_core::{service, storage};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

struct Workspace(PathBuf);
impl Workspace {
    fn new(label: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(format!(
                "recovery-{label}-{}-{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap()
            ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn install(&self) -> Value {
        let destination = self.0.join("user project");
        fs::create_dir_all(&destination).unwrap();
        service::execute(&self.0, json!({"action":"install","package":"hello","target":"claude-code","destination":destination,"dry_run":false})).unwrap()
    }
    fn skill(&self) -> PathBuf {
        self.0.join("user project/.claude/skills/hello/SKILL.md")
    }
    fn package(&self) -> PathBuf {
        let path = self.0.join("packages/claude-code/hello/skills/hello");
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join("SKILL.md"),
            b"Harmless first-party recovery fixture.\n",
        )
        .unwrap();
        path
    }
    fn rollback(&self, receipt: &Value) {
        service::execute(
            &self.0,
            json!({"action":"rollback","operation":receipt["operation"]}),
        )
        .unwrap();
    }
    fn receipts(&self) -> Value {
        service::execute(&self.0, json!({"action":"installed"})).unwrap()
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn permitted_binary_companion_does_not_make_history_unreadable() {
    let root = Workspace::new("large");
    let package = root.package();
    // The old pretty-printed byte array exceeds the 16 MiB read cap for this payload.
    let bytes = vec![255; 2 * 1024 * 1024];
    fs::write(package.join("companion.bin"), &bytes).unwrap();
    let receipt = root.install();
    let history = service::execute(&root.0, json!({"action":"history"})).unwrap();
    assert_eq!(history.as_array().unwrap().len(), 1);
    assert_eq!(
        fs::read(root.skill().with_file_name("companion.bin")).unwrap(),
        bytes
    );
    root.rollback(&receipt);
    assert!(!root.skill().exists());
    assert!(root.receipts().as_array().unwrap().is_empty());
}

#[test]
fn rollback_lifecycle_restores_receipt_and_payload_together() {
    let root = Workspace::new("lifecycle");
    root.package();
    let installed = root.install();
    let id = installed["id"].as_str().unwrap();
    let disabled = service::execute(&root.0, json!({"action":"disable","receipt":id})).unwrap();
    root.rollback(&disabled);
    assert!(root.skill().exists());
    assert_eq!(root.receipts()[0]["enabled"], true);
    let disabled = service::execute(&root.0, json!({"action":"disable","receipt":id})).unwrap();
    let enabled = service::execute(&root.0, json!({"action":"enable","receipt":id})).unwrap();
    root.rollback(&enabled);
    assert!(!root.skill().exists());
    assert_eq!(root.receipts()[0]["enabled"], false);
    root.rollback(&disabled);
    let uninstall = service::execute(&root.0, json!({"action":"uninstall","receipt":id})).unwrap();
    assert!(!root.skill().exists());
    assert!(root.receipts().as_array().unwrap().is_empty());
    root.rollback(&uninstall);
    assert!(root.skill().exists());
    assert_eq!(root.receipts()[0]["enabled"], true);
    root.rollback(&installed);
    assert!(!root.skill().exists());
    assert!(root.receipts().as_array().unwrap().is_empty());
}

#[test]
fn interrupted_receipt_commit_and_rollback_resume_on_next_request() {
    for state in ["applying", "rolling-back"] {
        let root = Workspace::new(state);
        root.package();
        let installed = root.install();
        let journal_path = root
            .0
            .join(".forge-state/history")
            .join(format!("{}.json", installed["operation"].as_str().unwrap()));
        let mut journal: Value = storage::read(&journal_path).unwrap();
        journal["state"] = json!(state);
        storage::write(&journal_path, &journal).unwrap();
        if state == "applying" {
            // Simulate the receipt not yet promoted after all payload writes.
            fs::remove_file(
                root.0
                    .join(".forge-state/receipts")
                    .join(format!("{}.json", installed["id"].as_str().unwrap())),
            )
            .unwrap();
        } else {
            // Simulate a payload already compensated while the receipt remains applied.
            fs::remove_file(root.skill()).unwrap();
        }
        service::execute(&root.0, json!({"action":"status"})).unwrap();
        assert!(!root.skill().exists());
        assert!(root.receipts().as_array().unwrap().is_empty());
        let recovered: Value = storage::read(&journal_path).unwrap();
        assert_eq!(recovered["state"], "rolled-back");
    }
}

#[test]
fn rollback_receipt_conflict_preserves_applied_payload() {
    let root = Workspace::new("receipt-conflict");
    root.package();
    let receipt = root.install();
    let path = root
        .0
        .join(".forge-state/receipts")
        .join(format!("{}.json", receipt["id"].as_str().unwrap()));
    let mut changed = receipt.clone();
    changed["package"] = json!("edited");
    storage::write(&path, &changed).unwrap();
    let result = service::execute(
        &root.0,
        json!({"action":"rollback","operation":receipt["operation"]}),
    );
    assert_eq!(result.unwrap_err().code, "conflict");
    assert!(root.skill().exists());
    assert_eq!(root.receipts()[0]["package"], "edited");
}
