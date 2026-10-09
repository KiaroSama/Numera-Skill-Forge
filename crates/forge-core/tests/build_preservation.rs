use forge_core::{
    build,
    model::{Accepted, Component, Kind, Lock, Package, Payload, Registry, Snapshot},
    registry, storage,
};
use std::{collections::BTreeMap, fs, path::PathBuf};

#[test]
fn build_rejects_missing_selection_and_preserves_modified_output() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.ci-work/tests")
        .join(format!("preserve-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let content = b"---\nname: hello\ndescription: Example\n---\nHello.\n";
    let cache = root.join(".forge-state/snapshots/demo/rev");
    fs::create_dir_all(cache.join("skills/hello")).unwrap();
    fs::write(cache.join("skills/hello/SKILL.md"), content).unwrap();
    fs::write(cache.join("LICENSE"), b"license").unwrap();
    let component = Component {
        id: "demo:skills/hello".into(),
        name: "hello".into(),
        kind: Kind::Skills,
        root: "skills/hello".into(),
        required: true,
        metadata: BTreeMap::new(),
    };
    let snapshot = Snapshot {
        source: "demo".into(),
        revision: "rev".into(),
        content_digest: "accepted".into(),
        license_digest: "license".into(),
        files: vec![
            Payload {
                path: "skills/hello/SKILL.md".into(),
                digest: storage::digest(content),
                size: content.len() as u64,
                executable: false,
            },
            Payload {
                path: "LICENSE".into(),
                digest: storage::digest(b"license"),
                size: 7,
                executable: false,
            },
        ],
        components: vec![component],
    };
    let mut registry = Registry::default();
    registry.sources.push(
        serde_json::from_value(
            serde_json::json!({"id":"demo","repository":"https://github.com/example/demo"}),
        )
        .unwrap(),
    );
    registry.packages.push(Package {
        id: "hello".into(),
        name: "Hello".into(),
        source: "demo".into(),
        components: vec!["demo:skills/hello".into(), "missing".into()],
        dependencies: vec![],
        targets: vec![],
        optional_exclusions: vec![],
    });
    registry::save(&root, &registry).unwrap();
    let mut lock = Lock {
        schema_version: 1,
        ..Default::default()
    };
    lock.sources.insert(
        "demo".into(),
        Accepted {
            snapshot,
            approved_digest: "accepted".into(),
            executable_approved: false,
        },
    );
    registry::save_lock(&root, &lock).unwrap();
    assert_eq!(
        build::build(&root, "hello", "claude-code")
            .unwrap_err()
            .code,
        "component-selection"
    );
    registry.packages[0].components.pop();
    registry::save(&root, &registry).unwrap();
    build::build(&root, "hello", "claude-code").unwrap();
    let output = root.join("packages/claude-code/hello/skills/hello/SKILL.md");
    fs::write(&output, b"user edit").unwrap();
    assert_eq!(
        build::build(&root, "hello", "claude-code")
            .unwrap_err()
            .code,
        "generated-conflict"
    );
    assert_eq!(fs::read(output).unwrap(), b"user edit");
    fs::remove_dir_all(root).unwrap();
}
