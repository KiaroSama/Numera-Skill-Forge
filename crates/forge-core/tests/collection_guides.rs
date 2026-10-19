use forge_core::{process, service, storage, transaction};
use serde_json::json;
use std::{fs, path::PathBuf};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn reviewed_guide_preserves_original_and_selects_no_sibling() {
    let f = Fixture(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(transaction::operation_id()),
    );
    let upstream = f.0.join("upstream");
    fs::create_dir_all(upstream.join("agents")).unwrap();
    let original = b"# Original guide\nExplain the user's problem before implementing.\n";
    fs::write(upstream.join("agents/demo.md"), original).unwrap();
    fs::write(upstream.join("agents/unrelated.md"), b"Not selected\n").unwrap();
    let license = b"MIT License\nPermission is hereby granted, free of charge\n";
    fs::write(upstream.join("agents/LICENSE"), license).unwrap();
    let git = |args: &[&str]| {
        let mut command = vec!["-C".into(), upstream.to_string_lossy().into_owned()];
        command.extend(args.iter().map(|a| a.to_string()));
        String::from_utf8(process::git(&command, 15).unwrap()).unwrap()
    };
    git(&["init", "-b", "main"]);
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-m",
        "fixture",
    ]);
    let mut registration = json!({"schema_version":1,"collections":[{"id":"guides","repository":format!("local:{}",upstream.display()),"branch":"main","reviewed_revision":git(&["rev-parse","HEAD"]).trim(),"skills":[{"id":"guide-demo","root":"agents","license":"MIT","license_path":"agents/LICENSE","license_digest":storage::digest(license),"reviewed_guide":{"name":"demo-guide","path":"agents/demo.md","source_digest":storage::digest(original),"adapter_version":1}}]}]});
    let registry = f.0.join("registry/collections.json");
    storage::write(&registry, &registration).unwrap();
    let result = service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(result["status"], "success", "{result}");
    let package = f.0.join("packages/claude-code/guide-demo");
    let skill = package.join("skills/demo-guide");
    assert_eq!(fs::read(skill.join("demo.md")).unwrap(), original);
    assert_eq!(fs::read(skill.join("LICENSE")).unwrap(), license);
    assert!(!skill.join("unrelated.md").exists());
    let entry = fs::read_to_string(skill.join("SKILL.md")).unwrap();
    assert!(entry.contains("[original guide](demo.md)"));
    assert!(entry.contains("Numera Skill Forge"));
    service::execute(&f.0, json!({"action":"collection-verify"})).unwrap();
    let accepted = fs::read(f.0.join("catalogs/collections.lock.json")).unwrap();
    service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        accepted
    );
    let approved = registration.clone();
    for invalid in [
        json!(null),
        json!({"name":"other","path":"agents/demo.md","source_digest":storage::digest(original),"adapter_version":1}),
    ] {
        registration["collections"][0]["skills"][0]["reviewed_guide"] = invalid;
        storage::write(&registry, &registration).unwrap();
        assert_eq!(
            service::execute(&f.0, json!({"action":"collection-verify"}))
                .unwrap_err()
                .code,
            "approval-stale"
        );
    }
    for (field, invalid) in [
        ("path", json!("agents/skill.md")),
        ("path", json!("elsewhere/demo.md")),
        ("adapter_version", json!(2)),
        ("source_digest", json!("not-a-digest")),
    ] {
        registration = approved.clone();
        registration["collections"][0]["skills"][0]["reviewed_guide"][field] = invalid;
        storage::write(&registry, &registration).unwrap();
        assert_eq!(
            service::execute(&f.0, json!({"action":"collection-sync","apply":true}))
                .unwrap_err()
                .code,
            "guide-review"
        );
        assert_eq!(
            fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
            accepted
        );
    }
    storage::write(&registry, &approved).unwrap();
    fs::write(skill.join("SKILL.md"), b"Consumer edit\n").unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-verify"}))
            .unwrap_err()
            .code,
        "generated-conflict"
    );
    fs::write(skill.join("SKILL.md"), entry.as_bytes()).unwrap();
    fs::remove_dir_all(&package).unwrap();
    let rebuilt = service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(rebuilt["status"], "success", "{rebuilt}");
    assert_eq!(fs::read(skill.join("demo.md")).unwrap(), original);
    assert_eq!(fs::read(skill.join("LICENSE")).unwrap(), license);
    fs::write(upstream.join("agents/demo.md"), b"Changed original guide\n").unwrap();
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-m",
        "changed",
    ]);
    let held = service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(held["status"], "partial");
    assert_eq!(held["skills"][0]["status"], "review-required", "{held}");
    assert_eq!(held["skills"][0]["code"], "guide-review", "{held}");
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        accepted
    );
    assert_eq!(fs::read(skill.join("demo.md")).unwrap(), original);
    assert_eq!(fs::read(skill.join("LICENSE")).unwrap(), license);
}
