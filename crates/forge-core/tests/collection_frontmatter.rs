use forge_core::{process, service, storage, transaction};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
impl Fixture {
    fn git(&self, args: &[&str]) -> String {
        let mut command = vec![
            "-C".into(),
            self.0.join("upstream").to_string_lossy().into_owned(),
        ];
        command.extend(args.iter().map(|v| v.to_string()));
        String::from_utf8(process::git(&command, 15).unwrap()).unwrap()
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
        service::execute(&self.0, json!({"action":"collection-sync","apply":true})).unwrap()
    }
}

#[test]
fn crlf_original_is_accepted_without_rewriting_bytes() {
    let f = Fixture(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(transaction::operation_id()),
    );
    let upstream = f.0.join("upstream");
    fs::create_dir_all(upstream.join("skills/demo/references")).unwrap();
    let original =
        b"---\r\nname: demo\r\ndescription: Original fixture\r\n---\r\nOriginal sentence.\r\n";
    fs::write(upstream.join("skills/demo/SKILL.md"), original).unwrap();
    fs::write(
        upstream.join("skills/demo/references/guide.md"),
        b"Original companion\r\n",
    )
    .unwrap();
    let license = b"MIT License\nPermission is hereby granted, free of charge\n";
    fs::write(upstream.join("LICENSE"), license).unwrap();
    f.git(&["init", "-b", "main"]);
    f.git(&["config", "core.autocrlf", "false"]);
    f.commit();
    storage::write(&f.0.join("registry/collections.json"), &json!({"schema_version":1,"collections":[{"id":"fixture","repository":format!("local:{}",upstream.display()),"branch":"main","reviewed_revision":f.git(&["rev-parse","HEAD"]).trim(),"skills":[{"id":"fixture-demo","root":"skills/demo","license":"MIT","license_path":"LICENSE","license_digest":storage::digest(license)}]}]})).unwrap();
    let snapshot = forge_core::discovery::inspect(&upstream, "fixture", "original").unwrap();
    assert_eq!(
        snapshot
            .components
            .iter()
            .find(|c| c.kind == forge_core::model::Kind::Skills)
            .unwrap()
            .metadata["description"],
        "Original fixture"
    );
    let result = f.sync();
    assert_eq!(result["status"], "success", "{result}");
    assert_eq!(
        fs::read(f.0.join("packages/claude-code/fixture-demo/skills/demo/SKILL.md")).unwrap(),
        original
    );
    service::execute(&f.0, json!({"action":"collection-verify"})).unwrap();
    service::execute(
        &f.0,
        json!({"action":"build","package":"fixture-demo","target":"claude-code"}),
    )
    .unwrap();
    let destination = f.0.join("consumer");
    fs::create_dir(&destination).unwrap();
    let request = json!({"action":"install","package":"fixture-demo","target":"claude-code","scope":"project","destination":destination,"dry_run":false});
    let receipt = service::execute(&f.0, request).unwrap();
    assert_eq!(
        fs::read(destination.join(".claude/skills/demo/SKILL.md")).unwrap(),
        original
    );
    assert_eq!(
        fs::read(destination.join(".claude/skills/demo/references/guide.md")).unwrap(),
        b"Original companion\r\n"
    );
    service::execute(&f.0, json!({"action":"uninstall","receipt":receipt["id"]})).unwrap();
    let lock_path = f.0.join("catalogs/collections.lock.json");
    let accepted = fs::read(&lock_path).unwrap();
    assert_eq!(f.sync()["skills"][0]["status"], "unchanged");
    assert_eq!(fs::read(&lock_path).unwrap(), accepted);
    let improved =
        b"---\r\nname: demo\r\ndescription: Original fixture\r\n---\r\nImproved sentence.\r\n";
    fs::write(upstream.join("skills/demo/SKILL.md"), improved).unwrap();
    f.commit();
    assert_eq!(f.sync()["skills"][0]["status"], "updated");
    let accepted = fs::read(&lock_path).unwrap();
    fs::write(
        upstream.join("skills/demo/SKILL.md"),
        b"---\r\nname: demo\r\ndescription: Changed fixture\r\n---\r\nImproved sentence.\r\n",
    )
    .unwrap();
    f.commit();
    assert_eq!(f.sync()["skills"][0]["status"], "review-required");
    assert_eq!(fs::read(&lock_path).unwrap(), accepted);
    assert_eq!(
        fs::read(f.0.join("packages/claude-code/fixture-demo/skills/demo/SKILL.md")).unwrap(),
        improved
    );
    for malformed in [
        "---suffix\nname: demo\ndescription: invalid\n---\nBody\n",
        "---\nname: demo\ndescription: invalid\n---suffix\nBody\n",
        "---\rname: demo\rdescription: invalid\r---\r",
        "---\nname: demo\ndescription: invalid\n",
    ] {
        fs::write(upstream.join("skills/demo/SKILL.md"), malformed).unwrap();
        assert_eq!(
            forge_core::discovery::inspect(&upstream, "fixture", "invalid")
                .err()
                .unwrap()
                .code,
            "frontmatter"
        );
        f.commit();
        assert_eq!(f.sync()["skills"][0]["code"], "frontmatter");
        assert_eq!(fs::read(&lock_path).unwrap(), accepted);
    }
}
