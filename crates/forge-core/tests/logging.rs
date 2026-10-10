use forge_core::{
    logging::{RunLog, redact},
    storage, transaction,
};
use std::{fs, path::PathBuf};
#[test]
fn per_run_logs_close_and_redact_json_and_token_literals() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.ci-work/tests")
        .join(transaction::operation_id());
    fs::create_dir_all(&root).unwrap();
    let first = RunLog::start("fixture", &root).unwrap();
    let path = first.path.clone();
    first.event("WARNING", "controlled", "normal warning");
    first.event(
        "ERROR",
        "controlled",
        "{\"api_key\": \"example-sensitive\"}",
    );
    let second = RunLog::start("fixture", &root).unwrap();
    assert_ne!(path, second.path);
    drop(first);
    drop(second);
    let bytes = storage::bytes(&path).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.contains("[WARNING] [controlled] normal warning"));
    assert!(text.contains("[shutdown]"));
    assert!(!text.contains("example-sensitive"));
    for value in [
        "{\"token\":\"example-sensitive\"}",
        "github_pat_example-sensitive",
        "https://user:example-sensitive@example.invalid",
    ] {
        assert!(!redact(value).contains("example-sensitive"));
    }
    fs::remove_dir_all(root).unwrap();
}
