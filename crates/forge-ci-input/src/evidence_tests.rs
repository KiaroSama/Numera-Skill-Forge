use crate::evidence;
use serde_json::json;
use std::{fs, path::PathBuf};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/input-evidence")
            .join(forge_core::transaction::operation_id());
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
fn evidence_is_utf8_create_only_and_never_overwrites_conflicting_outputs() {
    let f = Fixture::new();
    let record = json!({"outcome":"failed","reason":"α","cleanup_verified":true});
    let path = evidence::publish(&f.0, &record).unwrap();
    let original = fs::read(&path).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(std::str::from_utf8(&original).unwrap()).unwrap(),
        record
    );
    assert!(evidence::publish(&f.0, &json!({"outcome":"succeeded"})).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(evidence::publish(&f.0, &"x".repeat(1024 * 1024)).is_err());
}

#[test]
fn logs_are_collision_safe_utc_utf8_and_closed() {
    let f = Fixture::new();
    for _ in 0..2 {
        let mut log = evidence::Log::new(&f.0);
        log.event("INFO", "startup", "Controlled job started");
        log.event("ERROR", "shutdown", "controlled-failure α");
    }
    let files: Vec<_> = fs::read_dir(&f.0)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(files.len(), 2);
    for path in files {
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("forge-ci-input_")
        );
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("UTC] [INFO] [startup]"));
        assert!(text.contains("UTC] [ERROR] [shutdown] controlled-failure α"));
        fs::remove_file(path).unwrap();
    }
}
