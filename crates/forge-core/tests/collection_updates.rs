use forge_core::{process, service, storage};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

struct Fixture {
    root: PathBuf,
    upstream: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(format!(
                "updates-{}-{}-{}",
                std::process::id(),
                std::thread::current().name().unwrap_or("test"),
                chrono::Utc::now().timestamp_nanos_opt().unwrap()
            ));
        let upstream = root.join("upstream");
        fs::create_dir_all(upstream.join("skills/demo/scripts")).unwrap();
        fs::write(
            upstream.join("skills/demo/SKILL.md"),
            b"---\nname: demo\ndescription: Fixture\n---\nOriginal sentence.\n",
        )
        .unwrap();
        fs::write(
            upstream.join("skills/demo/LICENSE"),
            b"MIT License\nPermission is hereby granted, free of charge\n",
        )
        .unwrap();
        fs::write(
            upstream.join("skills/demo/scripts/helper.py"),
            b"print('never execute')\n",
        )
        .unwrap();
        let fixture = Self { root, upstream };
        fixture.git(&["init", "-b", "main"]);
        fixture.commit();
        let revision = fixture.git(&["rev-parse", "HEAD"]);
        storage::write(&fixture.root.join("registry/collections.json"),&json!({"schema_version":1,"collections":[{"id":"fixture","repository":format!("local:{}",fixture.upstream.display()),"branch":"main","reviewed_revision":revision.trim(),"skills":[{"id":"fixture-demo","root":"skills/demo","license":"MIT","license_path":"skills/demo/LICENSE","license_digest":storage::digest(&fs::read(fixture.upstream.join("skills/demo/LICENSE")).unwrap())}]}]})).unwrap();
        fixture
    }
    fn git(&self, args: &[&str]) -> String {
        let mut command = vec!["-C".into(), self.upstream.to_string_lossy().into_owned()];
        command.extend(args.iter().map(|v| v.to_string()));
        String::from_utf8(process::git(&command, 30).unwrap()).unwrap()
    }
    fn commit(&self) {
        self.git(&["add", "."]);
        self.git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "fixture",
        ]);
    }
    fn sync(&self) -> Value {
        service::execute(&self.root, json!({"action":"collection-sync","apply":true})).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn safe_prompt_updates_but_changed_script_license_and_structure_require_review() {
    let f = Fixture::new();
    assert_eq!(f.sync()["status"], "success");
    fs::write(
        f.upstream.join("skills/demo/SKILL.md"),
        b"---\nname: demo\ndescription: Fixture\n---\nImproved sentence.\n",
    )
    .unwrap();
    f.commit();
    assert_eq!(f.sync()["skills"][0]["status"], "updated");
    let lock = fs::read(f.root.join("catalogs/collections.lock.json")).unwrap();
    fs::write(
        f.upstream.join("skills/demo/scripts/helper.py"),
        b"print('changed runtime')\n",
    )
    .unwrap();
    f.commit();
    assert_eq!(f.sync()["skills"][0]["status"], "review-required");
    assert_eq!(
        fs::read(f.root.join("catalogs/collections.lock.json")).unwrap(),
        lock
    );
    fs::write(f.upstream.join("skills/demo/new.md"), b"new companion").unwrap();
    f.commit();
    assert_eq!(f.sync()["status"], "partial");
    fs::write(f.upstream.join("skills/demo/LICENSE"), b"Restricted rights").unwrap();
    f.commit();
    assert_eq!(f.sync()["skills"][0]["code"], "license-review");
    assert_eq!(
        fs::read(f.root.join("catalogs/collections.lock.json")).unwrap(),
        lock
    );
}
#[test]
fn clean_workspace_reconstructs_accepted_bytes_and_preserves_user_edits() {
    let f = Fixture::new();
    f.sync();
    let package = f.root.join("packages/claude-code/fixture-demo");
    fs::remove_dir_all(&package).unwrap();
    assert_eq!(f.sync()["status"], "success");
    let file = package.join("skills/demo/SKILL.md");
    fs::write(&file, b"user edit").unwrap();
    assert_eq!(f.sync()["skills"][0]["code"], "generated-conflict");
    assert_eq!(fs::read(file).unwrap(), b"user edit");
}

#[test]
fn separate_origins_keep_failed_content_while_healthy_collection_updates() {
    let f = Fixture::new();
    let second = f.root.join("second-upstream");
    fs::create_dir_all(second.join("skills/demo")).unwrap();
    for name in ["SKILL.md", "LICENSE"] {
        fs::copy(
            f.upstream.join("skills/demo").join(name),
            second.join("skills/demo").join(name),
        )
        .unwrap();
    }
    let git = |args: &[&str]| {
        let mut command = vec!["-C".into(), second.to_string_lossy().into_owned()];
        command.extend(args.iter().map(|v| v.to_string()));
        String::from_utf8(process::git(&command, 30).unwrap()).unwrap()
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
    let path = f.root.join("registry/collections.json");
    let mut registry: Value = storage::read(&path).unwrap();
    let mut collection = registry["collections"][0].clone();
    collection["id"] = json!("second");
    collection["repository"] = json!(format!("local:{}", second.display()));
    collection["reviewed_revision"] = json!(git(&["rev-parse", "HEAD"]).trim());
    collection["skills"][0]["id"] = json!("second-demo");
    registry["collections"]
        .as_array_mut()
        .unwrap()
        .push(collection);
    storage::write(&path, &registry).unwrap();
    assert_eq!(f.sync()["status"], "success");
    let retained = f
        .root
        .join("packages/claude-code/second-demo/skills/demo/SKILL.md");
    let original = fs::read(&retained).unwrap();
    let before: Value = storage::read(&f.root.join("catalogs/collections.lock.json")).unwrap();
    registry["collections"][1]["branch"] = json!("missing");
    storage::write(&path, &registry).unwrap();
    fs::write(
        f.upstream.join("skills/demo/SKILL.md"),
        b"---\nname: demo\ndescription: Fixture\n---\nImproved sentence.\n",
    )
    .unwrap();
    f.commit();
    let result = f.sync();
    assert_eq!(result["status"], "partial");
    assert_eq!(result["skills"][0]["status"], "updated");
    assert_eq!(result["skills"][1]["status"], "failed");
    assert_eq!(fs::read(&retained).unwrap(), original);
    let after: Value = storage::read(&f.root.join("catalogs/collections.lock.json")).unwrap();
    assert_eq!(
        after["entries"]["second/second-demo"],
        before["entries"]["second/second-demo"]
    );
    assert_eq!(after["entries"]["fixture/fixture-demo"]["version"], "1.0.1");
    let lock = fs::read(f.root.join("catalogs/collections.lock.json")).unwrap();
    assert_eq!(f.sync()["skills"][0]["status"], "unchanged");
    assert_eq!(
        fs::read(f.root.join("catalogs/collections.lock.json")).unwrap(),
        lock
    );
}

#[test]
fn native_delivery_preserves_original_acceptance_and_refuses_edited_metadata() {
    let f = Fixture::new();
    f.sync();
    let package = f.root.join("packages/claude-code/fixture-demo");
    let original = fs::read(package.join("skills/demo/SKILL.md")).unwrap();
    for descriptor in [
        ".codex-plugin/plugin.json",
        ".cursor-plugin/plugin.json",
        "plugin.json",
    ] {
        let manifest: Value = storage::read(&package.join(descriptor)).unwrap();
        assert_eq!(manifest["name"], "fixture-demo");
        assert_eq!(manifest["version"], "1.0.0");
    }
    for path in [
        ".claude-plugin/marketplace.json",
        ".cursor-plugin/marketplace.json",
    ] {
        let market: Value = storage::read(&f.root.join(path)).unwrap();
        assert_eq!(market["plugins"][0]["name"], "fixture-demo");
    }
    let accepted = fs::read(f.root.join("catalogs/collections.lock.json")).unwrap();
    assert_eq!(f.sync()["skills"][0]["status"], "unchanged");
    assert_eq!(
        fs::read(f.root.join("catalogs/collections.lock.json")).unwrap(),
        accepted
    );
    assert_eq!(
        fs::read(package.join("skills/demo/SKILL.md")).unwrap(),
        original
    );
    fs::write(package.join(".cursor-plugin/plugin.json"), b"user edit").unwrap();
    assert_eq!(f.sync()["skills"][0]["code"], "generated-conflict");
    assert_eq!(
        fs::read(package.join(".cursor-plugin/plugin.json")).unwrap(),
        b"user edit"
    );
}

#[test]
fn legacy_metadata_migration_is_offline_and_does_not_revise_original_identity() {
    let f = Fixture::new();
    f.sync();
    let path = f.root.join("catalogs/collections.lock.json");
    let mut lock: Value = storage::read(&path).unwrap();
    let entry = &mut lock["entries"]["fixture/fixture-demo"];
    let delivery = entry.as_object_mut().unwrap().remove("delivery").unwrap();
    for path in delivery.as_object().unwrap().keys() {
        fs::remove_file(f.root.join(path)).unwrap();
    }
    storage::write(&path, &lock).unwrap();
    let legacy = lock["entries"]["fixture/fixture-demo"].clone();
    service::execute(
        &f.root,
        json!({"action":"build","package":"fixture-demo","target":"hermes"}),
    )
    .unwrap();
    let migrated: Value = storage::read(&path).unwrap();
    let mut original = migrated["entries"]["fixture/fixture-demo"].clone();
    assert_eq!(
        original
            .as_object_mut()
            .unwrap()
            .remove("delivery")
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(original, legacy);
    let bytes = fs::read(&path).unwrap();
    service::execute(
        &f.root,
        json!({"action":"build","package":"fixture-demo","target":"hermes"}),
    )
    .unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::remove_file(f.root.join(".cursor-plugin/marketplace.json")).unwrap();
    assert_eq!(
        service::execute(&f.root, json!({"action":"collection-verify"}))
            .unwrap_err()
            .code,
        "catalog"
    );
    service::execute(
        &f.root,
        json!({"action":"build","package":"fixture-demo","target":"codex"}),
    )
    .unwrap();
    assert!(f.root.join(".cursor-plugin/marketplace.json").is_file());
}

#[test]
fn stale_legacy_approval_refuses_metadata_writes_and_other_packages_stay_visible() {
    let f = Fixture::new();
    f.sync();
    let path = f.root.join("catalogs/collections.lock.json");
    let mut lock: Value = storage::read(&path).unwrap();
    let delivery = lock["entries"]["fixture/fixture-demo"]
        .as_object_mut()
        .unwrap()
        .remove("delivery")
        .unwrap();
    for path in delivery.as_object().unwrap().keys() {
        fs::remove_file(f.root.join(path)).unwrap();
    }
    storage::write(&path, &lock).unwrap();
    let registry_path = f.root.join("registry/collections.json");
    let mut registry: Value = storage::read(&registry_path).unwrap();
    registry["collections"][0]["skills"][0]["license"] = json!("Apache-2.0");
    storage::write(&registry_path, &registry).unwrap();
    let before = fs::read(&path).unwrap();
    assert_eq!(
        service::execute(
            &f.root,
            json!({"action":"build","package":"fixture-demo","target":"codex"})
        )
        .unwrap_err()
        .code,
        "approval-stale"
    );
    assert_eq!(fs::read(path).unwrap(), before);
    assert!(
        !f.root
            .join("packages/claude-code/fixture-demo/plugin.json")
            .exists()
    );
    fs::write(
        f.root
            .join("packages/claude-code/fixture-demo/skills/demo/SKILL.md"),
        b"user edit",
    )
    .unwrap();
    let catalog = service::execute(&f.root, json!({"action":"catalog"})).unwrap();
    assert_eq!(catalog[0]["id"], "fixture-demo");
    assert_eq!(catalog[0]["error"]["code"], "integrity");
}

#[cfg(unix)]
#[test]
fn package_parent_links_cannot_bypass_collection_ownership() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    f.sync();
    fs::rename(f.root.join("packages"), f.root.join("outside-packages")).unwrap();
    symlink(f.root.join("outside-packages"), f.root.join("packages")).unwrap();
    assert!(service::execute(&f.root, json!({"action":"collection-verify"})).is_err());
    fs::remove_file(f.root.join("packages")).unwrap();
}

#[cfg(unix)]
#[test]
fn interrupted_promotion_stops_before_another_plugin_overwrites_recovery_state() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let path = f.root.join("registry/collections.json");
    let mut registry: Value = storage::read(&path).unwrap();
    let mut second = registry["collections"][0]["skills"][0].clone();
    second["id"] = json!("fixture-second");
    registry["collections"][0]["skills"]
        .as_array_mut()
        .unwrap()
        .push(second);
    storage::write(&path, &registry).unwrap();
    f.sync();
    let before = fs::read(f.root.join("catalogs/collections.lock.json")).unwrap();
    fs::write(
        f.upstream.join("skills/demo/SKILL.md"),
        b"---\nname: demo\ndescription: Fixture\n---\nImproved sentence.\n",
    )
    .unwrap();
    f.commit();
    let parent = f.root.join("packages/claude-code/fixture-demo/skills/demo");
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o555)).unwrap();
    let interrupted = service::execute(&f.root, json!({"action":"collection-sync","apply":true}));
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(interrupted.unwrap_err().code, "recovery-required");
    service::execute(&f.root, json!({"action":"status"})).unwrap();
    assert_eq!(
        fs::read(f.root.join("catalogs/collections.lock.json")).unwrap(),
        before
    );
}
