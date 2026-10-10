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
fn external_notices_and_template_approval_remain_bound_to_accepted_metadata() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.ci-work/tests")
        .join(transaction::operation_id());
    fs::create_dir_all(root.join("upstream/skills/demo/assets")).unwrap();
    let f = Fixture(root);
    let upstream = f.0.join("upstream");
    let git = |args: &[&str]| {
        let mut command = vec!["-C".into(), upstream.to_string_lossy().into_owned()];
        command.extend(args.iter().map(|v| v.to_string()));
        String::from_utf8(process::git(&command, 15).unwrap()).unwrap()
    };
    fs::write(
        upstream.join("skills/demo/SKILL.md"),
        b"---\nname: demo\ndescription: original\n---\nPrompt\n",
    )
    .unwrap();
    let template = b"API_KEY=example-placeholder\n";
    fs::write(upstream.join("skills/demo/assets/.env.example"), template).unwrap();
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
    let sha = git(&["rev-parse", "HEAD"]);
    let license = b"MIT License\nPermission is hereby granted, free of charge\n";
    storage::atomic(&f.0.join("registry/licenses/original.txt"), license).unwrap();
    let digest = storage::digest(license);
    let mut registration = json!({"schema_version":1,"collections":[{"id":"fixture","repository":format!("local:{}",upstream.display()),"branch":"main","reviewed_revision":sha.trim(),"skills":[{"id":"fixture-demo","root":"skills/demo","license":"MIT","license_path":"","license_digest":digest,"notices":[{"path":"registry/licenses/original.txt","digest":digest,"source":"https://github.com/example/original/blob/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/LICENSE","license":"MIT"}],"attribution":"Original fixture author","reviewed_templates":{"skills/demo/assets/.env.example":storage::digest(template)}}]}]});
    storage::write(&f.0.join("registry/collections.json"), &registration).unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap()["status"],
        "success"
    );
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-verify"})).unwrap()["verified"],
        true
    );
    let original = registration.clone();
    registration["collections"][0]["skills"][0]["attribution"] = json!("changed author");
    storage::write(&f.0.join("registry/collections.json"), &registration).unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-verify"}))
            .unwrap_err()
            .code,
        "approval-stale"
    );
    registration = original;
    registration["collections"][0]["skills"][0]["reviewed_templates"] = json!({});
    storage::write(&f.0.join("registry/collections.json"), &registration).unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-verify"}))
            .unwrap_err()
            .code,
        "approval-stale"
    );
    registration["collections"][0]["skills"][0]["reviewed_templates"] =
        json!({"skills/demo/.ignoreme/.env.example":storage::digest(template)});
    storage::write(&f.0.join("registry/collections.json"), &registration).unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-sync"}))
            .unwrap_err()
            .code,
        "protected-payload"
    );
}
