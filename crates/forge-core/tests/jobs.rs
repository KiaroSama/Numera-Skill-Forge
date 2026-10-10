use forge_core::{service, storage, transaction};
use serde_json::json;
use std::{fs, path::PathBuf};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(transaction::operation_id());
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn invalid_job_values_are_rejected_before_creating_durable_records() {
    let f = Fixture::new();
    for request in [
        json!({"action":"discover","source":"https://user:example.invalid@example.com"}),
        json!({"action":"build","package":"valid","target":"../outside"}),
        json!({"action":"sync","apply":"true"}),
    ] {
        assert!(service::execute(&f.0, json!({"action":"job-run","request":request})).is_err());
        assert!(!f.0.join(".forge-state/jobs").exists());
    }
}
#[cfg(unix)]
#[test]
fn linked_job_state_ancestor_is_rejected_before_external_write() {
    let f = Fixture::new();
    let external = Fixture::new();
    std::os::unix::fs::symlink(&external.0, f.0.join(".forge-state")).unwrap();
    let error = service::execute(
        &f.0,
        json!({"action":"job-run","request":{"action":"discover","source":"missing"}}),
    )
    .unwrap_err();
    assert_eq!(error.code, "unsafe-link");
    assert!(!external.0.join("jobs").exists());
}
#[test]
fn persisted_job_errors_retry_and_owner_exit_are_not_false_success() {
    let f = Fixture::new();
    let request = json!({"action":"job-run","job":"missing-source","timeout":30,"request":{"action":"discover","source":"missing","offline":true}});
    let failed = service::execute(&f.0, request).unwrap();
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["error"]["code"], "source");
    let retry =
        service::execute(&f.0, json!({"action":"job-retry","job":"missing-source"})).unwrap();
    assert_ne!(retry["id"], failed["id"]);
    assert_eq!(retry["status"], "failed");
    storage::write(
        &f.0.join(".forge-state/jobs/exited.json"),
        &json!({"id":"exited","status":"running"}),
    )
    .unwrap();
    let exited = service::execute(&f.0, json!({"action":"job-status","job":"exited"})).unwrap();
    assert_eq!(exited["status"], "interrupted");
    assert_eq!(
        service::execute(&f.0, json!({"action":"job-cancel","job":"exited"})).unwrap()["cancel_requested"],
        false
    );
    assert_eq!(
        service::execute(
            &f.0,
            json!({"action":"job-run","request":{"action":"publish"}})
        )
        .unwrap_err()
        .code,
        "job-action"
    );
}
#[test]
fn support_bundle_omits_logs_by_default_and_retention_rejects_invalid_bounds() {
    let f = Fixture::new();
    storage::atomic(
        &f.0.join(".forge-state/logs/test.log"),
        b"token=example password=example\n",
    )
    .unwrap();
    let bundle = service::execute(&f.0, json!({"action":"support-bundle"})).unwrap();
    let content: serde_json::Value =
        storage::read(&PathBuf::from(bundle["path"].as_str().unwrap())).unwrap();
    assert_eq!(content["logs"], json!([]));
    assert_eq!(
        service::execute(&f.0, json!({"action":"log-retention","days":0}))
            .unwrap_err()
            .code,
        "retention"
    );
    let bundle =
        service::execute(&f.0, json!({"action":"support-bundle","include_logs":true})).unwrap();
    let bytes = storage::bytes(&PathBuf::from(bundle["path"].as_str().unwrap())).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(!text.contains("token=example"));
    assert!(!text.contains("password=example"));
}
