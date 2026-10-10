use forge_core::{
    model::{Accepted, Component, Kind, Package, Payload, Snapshot, Source},
    registry, service, storage, transaction,
};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::PathBuf};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(transaction::operation_id());
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn source(&self, id: &str, skill: &str, body: &[u8], executable: bool) {
        let source: Source = serde_json::from_value(
            json!({"id":id,"repository":format!("local:{}",self.0.display())}),
        )
        .unwrap();
        registry::add_source(&self.0, source).unwrap();
        let relative = format!("skills/{skill}/SKILL.md");
        let revision = "a".repeat(40);
        let path = self
            .0
            .join(format!(".forge-state/snapshots/{id}/{revision}/{relative}"));
        storage::atomic(&path, body).unwrap();
        let files = vec![Payload {
            path: relative,
            digest: storage::digest(body),
            size: body.len() as u64,
            executable,
        }];
        let digest = storage::digest(&storage::encoded(&files).unwrap());
        let snapshot = Snapshot {
            source: id.into(),
            revision,
            content_digest: digest.clone(),
            license_digest: "test".into(),
            files,
            components: vec![Component {
                id: format!("{id}:skills/{skill}"),
                name: skill.into(),
                kind: Kind::Skills,
                root: format!("skills/{skill}"),
                required: true,
                metadata: BTreeMap::new(),
            }],
        };
        let mut lock = registry::lock(&self.0).unwrap();
        lock.sources.insert(
            id.into(),
            Accepted {
                snapshot,
                approved_digest: digest,
                executable_approved: true,
            },
        );
        registry::save_lock(&self.0, &lock).unwrap();
    }
    fn package(&self, id: &str, source: &str, dependencies: Vec<String>) {
        registry::add_package(
            &self.0,
            Package {
                id: id.into(),
                name: id.into(),
                source: source.into(),
                components: vec![],
                dependencies,
                targets: vec!["claude-code".into()],
                optional_exclusions: vec![],
            },
        )
        .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn closure_combines_sources_and_lock_rollback_restores_build_identity() {
    let f = Fixture::new();
    f.source("first", "hello", b"first", true);
    f.source("second", "world", b"second", false);
    f.package("dependency", "first", vec![]);
    f.package("combined", "second", vec!["dependency".into()]);
    let lock = fs::read(f.0.join("forge.lock.json")).unwrap();
    let result = service::execute(
        &f.0,
        json!({"action":"build","package":"combined","target":"claude-code"}),
    )
    .unwrap();
    let built = f.0.join("packages/claude-code/combined");
    assert_eq!(
        fs::read(built.join("skills/hello/SKILL.md")).unwrap(),
        b"first"
    );
    assert_eq!(
        fs::read(built.join("skills/world/SKILL.md")).unwrap(),
        b"second"
    );
    let provenance: serde_json::Value = storage::read(&built.join("provenance.json")).unwrap();
    assert_eq!(provenance["sources"].as_array().unwrap().len(), 2);
    assert_eq!(provenance["file_modes"]["skills/hello/SKILL.md"], 493);
    service::execute(
        &f.0,
        json!({"action":"rollback","operation":result["operation"]}),
    )
    .unwrap();
    assert_eq!(fs::read(f.0.join("forge.lock.json")).unwrap(), lock);
    assert!(!built.join("skills/hello/SKILL.md").exists());
}
#[test]
fn dependency_payload_collision_refuses_before_promotion() {
    let f = Fixture::new();
    f.source("first", "same", b"first", false);
    f.source("second", "same", b"second", false);
    f.package("dependency", "first", vec![]);
    f.package("combined", "second", vec!["dependency".into()]);
    let lock = fs::read(f.0.join("forge.lock.json")).unwrap();
    let error = service::execute(
        &f.0,
        json!({"action":"build","package":"combined","target":"claude-code"}),
    )
    .unwrap_err();
    assert_eq!(error.code, "package-collision");
    assert_eq!(fs::read(f.0.join("forge.lock.json")).unwrap(), lock);
    assert!(!f.0.join("packages/claude-code/combined").exists());
}
