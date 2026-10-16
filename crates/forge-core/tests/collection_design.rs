use forge_core::{process, service, storage, transaction};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn reviewed_design_is_installable_without_rewriting_originals() {
    let f = Fixture(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(transaction::operation_id()),
    );
    let upstream = f.0.join("upstream");
    fs::create_dir_all(upstream.join("design-md/demo")).unwrap();
    let original = b"# Original design\nUse readable spacing.\n";
    fs::write(upstream.join("design-md/demo/DESIGN.md"), original).unwrap();
    fs::write(
        upstream.join("design-md/demo/README.md"),
        b"Original companion\n",
    )
    .unwrap();
    let license = b"MIT License\nPermission is hereby granted, free of charge\n";
    fs::write(upstream.join("LICENSE"), license).unwrap();
    let git = |args: &[&str]| {
        let mut a = vec!["-C".into(), upstream.to_string_lossy().into_owned()];
        a.extend(args.iter().map(|s| s.to_string()));
        String::from_utf8(process::git(&a, 15).unwrap()).unwrap()
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
    let approval = json!({"name":"voltagent-design-demo","source_digest":storage::digest(original),"adapter_version":1});
    let mut registry = json!({"schema_version":1,"collections":[{"id":"fixture","repository":format!("local:{}",upstream.display()),"branch":"main","reviewed_revision":git(&["rev-parse","HEAD"]).trim(),"skills":[{"id":"design-demo","root":"design-md/demo","license":"MIT","license_path":"LICENSE","license_digest":storage::digest(license),"reviewed_design":approval}]}]});
    let registration = f.0.join("registry/collections.json");
    storage::write(&registration, &registry).unwrap();
    let request = |value| service::execute(&f.0, value).unwrap();
    let first = request(json!({"action":"collection-sync","apply":true}));
    assert_eq!(first["status"], "success", "{first}");
    let package = f.0.join("packages/claude-code/design-demo");
    let skill = package.join("skills/voltagent-design-demo");
    assert_eq!(fs::read(skill.join("DESIGN.md")).unwrap(), original);
    assert_eq!(
        fs::read(skill.join("README.md")).unwrap(),
        b"Original companion\n"
    );
    let entry = fs::read(skill.join("SKILL.md")).unwrap();
    let text = std::str::from_utf8(&entry).unwrap();
    assert!(text.contains("name: voltagent-design-demo\n"));
    assert!(text.contains("[DESIGN.md](DESIGN.md)"));
    assert!(text.contains("Numera Skill Forge"));
    assert!(text.contains("third-party"));
    let manifest: Value = storage::read(&package.join("plugin.json")).unwrap();
    assert!(
        manifest["description"]
            .as_str()
            .unwrap()
            .contains("design adapter")
    );
    let lock = fs::read(f.0.join("catalogs/collections.lock.json")).unwrap();
    request(json!({"action":"collection-verify"}));
    request(json!({"action":"collection-sync","apply":true}));
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        lock
    );
    let accepted: Value = serde_json::from_slice(&lock).unwrap();
    assert_eq!(
        accepted["entries"]["fixture/design-demo"]["files"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        accepted["entries"]["fixture/design-demo"]["reviewed_design"],
        approval
    );
    assert_eq!(
        request(json!({"action":"catalog"}))[0]["name"],
        "voltagent-design-demo"
    );
    let destination = f.0.join("consumer");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("unrelated.txt"), b"user data").unwrap();
    let mut install = json!({"action":"install","package":"design-demo","target":"claude-code","destination":destination,"dry_run":true});
    request(install.clone());
    assert!(!destination.join(".claude").exists());
    install["dry_run"] = json!(false);
    let receipt = request(install);
    let installed = destination.join(".claude/skills/voltagent-design-demo");
    assert_eq!(fs::read(installed.join("SKILL.md")).unwrap(), entry);
    assert_eq!(fs::read(installed.join("DESIGN.md")).unwrap(), original);
    request(json!({"action":"disable","receipt":receipt["id"]}));
    assert!(!installed.join("SKILL.md").exists());
    request(json!({"action":"enable","receipt":receipt["id"]}));
    request(json!({"action":"uninstall","receipt":receipt["id"]}));
    assert_eq!(
        fs::read(destination.join("unrelated.txt")).unwrap(),
        b"user data"
    );
    let good = registry.clone();
    for changed in [
        json!(null),
        json!({"name":"other-name","source_digest":storage::digest(original),"adapter_version":1}),
        json!({"name":"voltagent-design-demo","source_digest":"0".repeat(64),"adapter_version":1}),
        json!({"name":"voltagent-design-demo","source_digest":storage::digest(original),"adapter_version":2}),
    ] {
        registry["collections"][0]["skills"][0]["reviewed_design"] = changed;
        storage::write(&registration, &registry).unwrap();
        assert!(service::execute(&f.0, json!({"action":"collection-verify"})).is_err());
        assert_eq!(
            fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
            lock
        );
    }
    storage::write(&registration, &good).unwrap();
    fs::write(skill.join("SKILL.md"), b"edited entry").unwrap();
    assert!(service::execute(&f.0, json!({"action":"collection-verify"})).is_err());
    fs::write(skill.join("SKILL.md"), &entry).unwrap();
    fs::remove_dir_all(&package).unwrap();
    assert_eq!(
        request(json!({"action":"collection-sync","apply":true}))["status"],
        "success"
    );
    assert_eq!(fs::read(skill.join("SKILL.md")).unwrap(), entry);
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        lock
    );
    fs::write(
        upstream.join("design-md/demo/DESIGN.md"),
        b"Changed source\n",
    )
    .unwrap();
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
    assert_ne!(
        request(json!({"action":"collection-sync","apply":true}))["status"],
        "success"
    );
    assert_eq!(fs::read(skill.join("DESIGN.md")).unwrap(), original);
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        lock
    );
}
