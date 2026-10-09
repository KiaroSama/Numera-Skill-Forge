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
                "collection-{}-{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap()
            ));
        let upstream = root.join("upstream");
        fs::create_dir_all(upstream.join("skills/demo/scripts")).unwrap();
        fs::write(
            upstream.join("skills/demo/SKILL.md"),
            b"---\nname: demo\ndescription: Original fixture\n---\nOriginal prompt.\n",
        )
        .unwrap();
        fs::write(upstream.join("skills/demo/LICENSE"), b"MIT License\nCopyright Fixture\nPermission is hereby granted, free of charge, to any person obtaining a copy\n").unwrap();
        fs::write(
            upstream.join("skills/demo/scripts/helper.sh"),
            b"#!/bin/sh\nprintf should-never-run > sentinel\n",
        )
        .unwrap();
        let fixture = Self { root, upstream };
        fixture.git(&["init", "-b", "main"]);
        fixture.git(&["add", "."]);
        fixture.git(&[
            "update-index",
            "--chmod=+x",
            "skills/demo/scripts/helper.sh",
        ]);
        fixture.commit();
        let revision = fixture.git(&["rev-parse", "HEAD"]);
        storage::write(&fixture.root.join("registry/collections.json"), &json!({
            "schema_version":1,"collections":[{"id":"demo","repository":format!("local:{}",fixture.upstream.display()),"branch":"main","reviewed_revision":revision.trim(),"skills":[{"id":"demo-skill","root":"skills/demo","license":"MIT","license_path":"skills/demo/LICENSE","license_digest":storage::digest(&fs::read(fixture.upstream.join("skills/demo/LICENSE")).unwrap())}]}]
        })).unwrap();
        fixture
    }
    fn git(&self, args: &[&str]) -> String {
        let mut command = vec![
            "-C".to_owned(),
            self.upstream.to_string_lossy().into_owned(),
        ];
        command.extend(args.iter().map(|v| v.to_string()));
        String::from_utf8(process::git(&command, 30).unwrap()).unwrap()
    }
    fn commit(&self) {
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
fn collection_preserves_complete_script_payload_without_execution() {
    let fixture = Fixture::new();
    let first = fixture.sync();
    assert_eq!(first["status"], "success");
    let package = fixture.root.join("packages/claude-code/demo-skill");
    assert_eq!(
        fs::read(package.join("skills/demo/scripts/helper.sh")).unwrap(),
        b"#!/bin/sh\nprintf should-never-run > sentinel\n"
    );
    assert!(!fixture.root.join("sentinel").exists());
    assert!(!fixture.upstream.join("sentinel").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            fs::metadata(package.join("skills/demo/scripts/helper.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111,
            0
        );
    }
    let lock = fs::read(fixture.root.join("catalogs/collections.lock.json")).unwrap();
    fixture.sync();
    assert_eq!(
        fs::read(fixture.root.join("catalogs/collections.lock.json")).unwrap(),
        lock
    );
    let verified = service::execute(&fixture.root, json!({"action":"collection-verify"})).unwrap();
    assert_eq!(verified["verified"], true);
}
