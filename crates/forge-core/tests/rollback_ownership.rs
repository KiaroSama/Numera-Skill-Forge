use forge_core::{service, storage, transaction};
use serde_json::json;
use std::{fs, path::PathBuf};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(transaction::operation_id());
        fs::create_dir_all(root.join("project")).unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn rollback_refuses_later_shared_owners_before_reverting_receipt_or_payload() {
    let f = Fixture::new();
    for id in ["first", "second"] {
        storage::atomic(
            &f.0.join(format!("packages/claude-code/{id}/skills/shared/SKILL.md")),
            b"original",
        )
        .unwrap();
    }
    let destination = f.0.join("project");
    let first=service::execute(&f.0,json!({"action":"install","package":"first","target":"claude-code","destination":destination,"dry_run":false})).unwrap();
    service::execute(&f.0,json!({"action":"install","package":"second","target":"claude-code","destination":destination,"dry_run":false})).unwrap();
    assert_eq!(
        service::execute(
            &f.0,
            json!({"action":"rollback","operation":first["operation"]})
        )
        .unwrap_err()
        .code,
        "shared-rollback"
    );
    assert_eq!(
        fs::read(destination.join(".claude/skills/shared/SKILL.md")).unwrap(),
        b"original"
    );
    assert_eq!(forge_core::install::receipts(&f.0).unwrap().len(), 2);
}
#[cfg(unix)]
#[test]
fn rollback_permission_conflict_preserves_receipt_and_atomic_mode() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let destination = f.0.join("project");
    let operation = transaction::apply_with_receipt_modes(
        &f.0,
        &destination,
        "mode-operation",
        vec![transaction::Change {
            path: "owned.sh".into(),
            before: None,
            after: Some(b"original".to_vec()),
        }],
        transaction::Change {
            path: "receipt.json".into(),
            before: None,
            after: Some(b"owned".to_vec()),
        },
        &[("owned.sh".into(), 0o755)].into(),
    )
    .unwrap();
    let path = destination.join("owned.sh");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
        0o755
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        transaction::rollback(&f.0, &operation).unwrap_err().code,
        "mode-conflict"
    );
    assert_eq!(fs::read(f.0.join("receipt.json")).unwrap(), b"owned");
    assert_eq!(fs::read(path).unwrap(), b"original");
}
