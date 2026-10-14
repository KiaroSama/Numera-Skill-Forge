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
fn exact_public_reference_approval_preserves_private_boundaries() {
    let f = Fixture(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(transaction::operation_id()),
    );
    let upstream = f.0.join("upstream");
    fs::create_dir_all(upstream.join("skills/demo/references")).unwrap();
    let document = b"# Public secret API guide\nUse API_KEY=placeholder only.\n";
    fs::write(upstream.join("skills/demo/references/secrets.md"), document).unwrap();
    fs::write(
        upstream.join("skills/demo/SKILL.md"),
        b"---\nname: demo\ndescription: original\n---\nPrompt\n",
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
    let mut registration = json!({"schema_version":1,"collections":[{"id":"fixture","repository":format!("local:{}",upstream.display()),"branch":"main","reviewed_revision":git(&["rev-parse","HEAD"]).trim(),"skills":[{"id":"fixture-demo","root":"skills/demo","license":"MIT","license_path":"LICENSE","license_digest":storage::digest(license),"reviewed_documents":{"skills/demo/references/secrets.md":storage::digest(document)}}]}]});
    storage::write(&f.0.join("registry/collections.json"), &registration).unwrap();
    let result = service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(result["status"], "success", "{result}");
    let copied =
        f.0.join("packages/claude-code/fixture-demo/skills/demo/references/secrets.md");
    assert_eq!(fs::read(&copied).unwrap(), document);
    service::execute(&f.0, json!({"action":"collection-verify"})).unwrap();
    let accepted = fs::read(f.0.join("catalogs/collections.lock.json")).unwrap();
    service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        accepted
    );
    let original = registration.clone();
    for bad in [
        json!({}),
        json!({"skills/demo/references/secrets.md":"0".repeat(64)}),
    ] {
        registration["collections"][0]["skills"][0]["reviewed_documents"] = bad;
        storage::write(&f.0.join("registry/collections.json"), &registration).unwrap();
        assert_eq!(
            service::execute(&f.0, json!({"action":"collection-verify"}))
                .unwrap_err()
                .code,
            "approval-stale"
        );
        assert_eq!(fs::read(&copied).unwrap(), document);
    }
    for path in [
        "secrets.md",
        "skills/demo/secrets.md",
        "skills/.ai/references/secrets.md",
        "skills/demo/references/.ignoreme/secrets.md",
    ] {
        registration = original.clone();
        registration["collections"][0]["skills"][0]["reviewed_documents"] =
            json!({path:storage::digest(document)});
        storage::write(&f.0.join("registry/collections.json"), &registration).unwrap();
        assert_eq!(
            service::execute(&f.0, json!({"action":"collection-sync","apply":true}))
                .unwrap_err()
                .code,
            "protected-payload"
        );
    }
    storage::write(&f.0.join("registry/collections.json"), &original).unwrap();
    fs::write(
        upstream.join("skills/demo/references/secrets.md"),
        b"changed public guide",
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
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap()["status"],
        "partial"
    );
    assert_eq!(fs::read(&copied).unwrap(), document);
}
