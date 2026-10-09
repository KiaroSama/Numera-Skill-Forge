use forge_core::{service, storage, transaction};
use serde_json::json;
use std::{fs, path::PathBuf};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(format!(
                "case-{}-{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap()
            ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn data_registration_rejects_duplicate_source_without_losing_registry() {
    let root = Workspace::new();
    let request =
        json!({"action":"source-add","id":"demo","repository":"https://github.com/example/demo"});
    service::execute(&root.0, request.clone()).unwrap();
    assert_eq!(
        service::execute(&root.0, request).unwrap_err().code,
        "duplicate-id"
    );
    let result = service::execute(&root.0, json!({"action":"source-list"})).unwrap();
    assert_eq!(result.as_array().unwrap().len(), 1);
    assert_eq!(result[0]["enabled"], false);
}

#[test]
fn installation_disable_enable_uninstall_preserves_user_edits() {
    let root = Workspace::new();
    let destination = root.0.join("user project سلام");
    fs::create_dir(&destination).unwrap();
    let package = root.0.join("packages/claude-code/hello/skills/hello");
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("SKILL.md"),
        b"---\nname: hello\ndescription: Test skill\n---\nSay hello.\n",
    )
    .unwrap();
    let request = json!({"action":"install","package":"hello","target":"claude-code","destination":destination,"dry_run":true});
    let preview = service::execute(&root.0, request.clone()).unwrap();
    assert_eq!(preview["dry_run"], true);
    assert!(!destination.join(".claude").exists());
    let mut apply = request;
    apply["dry_run"] = json!(false);
    let receipt = service::execute(&root.0, apply.clone()).unwrap();
    let id = receipt["id"].as_str().unwrap();
    let file = destination.join(".claude/skills/hello/SKILL.md");
    assert!(file.exists());
    service::execute(&root.0, apply).unwrap();
    service::execute(&root.0, json!({"action":"disable","receipt":id})).unwrap();
    assert!(!file.exists());
    service::execute(&root.0, json!({"action":"enable","receipt":id})).unwrap();
    assert!(file.exists());
    fs::write(&file, b"User's later edit").unwrap();
    assert_eq!(
        service::execute(&root.0, json!({"action":"uninstall","receipt":id}))
            .unwrap_err()
            .code,
        "conflict"
    );
    assert_eq!(fs::read(&file).unwrap(), b"User's later edit");
}

#[test]
fn interrupted_operation_recovers_previous_content() {
    let root = Workspace::new();
    let file = root.0.join("owned.txt");
    fs::write(&file, b"new").unwrap();
    let journal = transaction::Journal {
        id: "op-1".into(),
        destination: root.0.clone(),
        state: "applying".into(),
        changes: vec![transaction::Change {
            path: "owned.txt".into(),
            before: Some(b"old".to_vec()),
            after: Some(b"new".to_vec()),
        }],
    };
    storage::write(&root.0.join(".forge-state/history/op-1.json"), &journal).unwrap();
    service::execute(&root.0, json!({"action":"status"})).unwrap();
    assert_eq!(fs::read(file).unwrap(), b"old");
}
