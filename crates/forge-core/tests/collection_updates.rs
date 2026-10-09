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
