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
fn reviewed_name_changes_only_name_and_retains_source_identity() {
    let f = Fixture(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(transaction::operation_id()),
    );
    let upstream = f.0.join("upstream");
    fs::create_dir_all(upstream.join("skills/demo/references")).unwrap();
    let original = b"---\nname: Original Demo\ndescription: Original description\n---\n# Original Demo\nKeep all body bytes.\n";
    let expected = b"---\nname: original-demo\ndescription: Original description\n---\n# Original Demo\nKeep all body bytes.\n";
    fs::write(upstream.join("skills/demo/SKILL.md"), original).unwrap();
    fs::write(
        upstream.join("skills/demo/references/guide.md"),
        b"Original companion\n",
    )
    .unwrap();
    let license = b"MIT License\nPermission is hereby granted, free of charge\n";
    fs::write(upstream.join("LICENSE"), license).unwrap();
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
    let mut registration = json!({"schema_version":1,"collections":[{"id":"fixture","repository":format!("local:{}",upstream.display()),"branch":"main","reviewed_revision":git(&["rev-parse","HEAD"]).trim(),"skills":[{"id":"fixture-demo","root":"skills/demo","license":"MIT","license_path":"LICENSE","license_digest":storage::digest(license),"reviewed_name":{"source_name":"Original Demo","name":"original-demo","source_digest":storage::digest(original)}}]}]});
    let registry = f.0.join("registry/collections.json");
    storage::write(&registry, &registration).unwrap();
    fs::create_dir_all(f.0.join("registry/licenses")).unwrap();
    fs::write(f.0.join("registry/licenses/original.txt"), license).unwrap();
    registration["collections"][0]["skills"][0]["notices"] = json!([{
        "path":"registry/licenses/original.txt","digest":storage::digest(license),
        "source":"https://github.com/example/fixture/blob/0000000000000000000000000000000000000000/LICENSE","license":"MIT"
    }]);
    storage::write(&registry, &registration).unwrap();
    let result = service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(result["status"], "success", "{result}");
    let package = f.0.join("packages/claude-code/fixture-demo");
    assert_eq!(
        fs::read(package.join("skills/original-demo/SKILL.md")).unwrap(),
        expected
    );
    assert_eq!(
        fs::read(package.join("upstream-notices/source-entry.md")).unwrap(),
        original
    );
    assert_eq!(
        fs::read(package.join("skills/original-demo/references/guide.md")).unwrap(),
        b"Original companion\n"
    );
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
        json!({"source_name":"Original Demo","name":"other-name","source_digest":storage::digest(original)}),
        json!({"source_name":"Original Demo","name":"original-demo","source_digest":"0".repeat(64)}),
    ] {
        registration["collections"][0]["skills"][0]["reviewed_name"] = invalid;
        storage::write(&registry, &registration).unwrap();
        assert_eq!(
            service::execute(&f.0, json!({"action":"collection-verify"}))
                .unwrap_err()
                .code,
            "approval-stale"
        );
        assert_eq!(
            fs::read(package.join("skills/original-demo/SKILL.md")).unwrap(),
            expected
        );
    }
    storage::write(&registry, &approved).unwrap();
    fs::remove_dir_all(&package).unwrap();
    let rebuilt = service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(rebuilt["status"], "success", "{rebuilt}");
    assert_eq!(
        fs::read(package.join("skills/original-demo/SKILL.md")).unwrap(),
        expected
    );
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        accepted
    );
    let mut changed = original.to_vec();
    changed.extend(b"Changed body\n");
    fs::write(upstream.join("skills/demo/SKILL.md"), changed).unwrap();
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
    assert_eq!(held["skills"][0]["code"], "adaptation-review");
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        accepted
    );
    assert_eq!(
        fs::read(package.join("skills/original-demo/SKILL.md")).unwrap(),
        expected
    );
}
