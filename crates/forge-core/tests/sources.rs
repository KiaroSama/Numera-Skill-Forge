use forge_core::{service, storage};
use serde_json::json;
use std::{fs, path::PathBuf};

#[test]
fn approved_git_skill_build_is_deterministic_and_data_driven() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.ci-work/tests")
        .join(format!("source-{}", std::process::id()));
    if root.exists() {
        panic!("fixture directory already exists");
    }
    fs::create_dir_all(&root).unwrap();
    let upstream = root.join("upstream");
    fs::create_dir(&upstream).unwrap();
    let run = |args: &[&str]| {
        let mut command = vec!["-C".into(), upstream.to_string_lossy().into_owned()];
        command.extend(args.iter().map(|arg| arg.to_string()));
        forge_core::process::git(&command, 30).unwrap();
    };
    run(&["init", "-b", "trunk"]);
    run(&["config", "user.name", "Fixture"]);
    run(&["config", "user.email", "fixture@example.invalid"]);
    fs::create_dir_all(upstream.join("skills/hello")).unwrap();
    fs::write(
        upstream.join("LICENSE"),
        "First-party test fixture, GPL-3.0-only.\n",
    )
    .unwrap();
    fs::write(
        upstream.join("skills/hello/SKILL.md"),
        "---\nname: hello\ndescription: Harmless hello skill\n---\nSay hello.\n",
    )
    .unwrap();
    fs::create_dir_all(upstream.join("skills/hello/.codex")).unwrap();
    fs::create_dir_all(upstream.join("skills/hello/.claude")).unwrap();
    fs::write(
        upstream.join("skills/hello/Secrets.md"),
        b"Excluded first-party marker",
    )
    .unwrap();
    fs::write(
        upstream.join("skills/hello/.codex/auth.json"),
        b"{\"excluded\":true}",
    )
    .unwrap();
    fs::write(
        upstream.join("skills/hello/.claude/settings.local.json"),
        b"{\"excluded\":true}",
    )
    .unwrap();
    fs::write(
        upstream.join("skills/hello/.codex/public.json"),
        b"{\"retained\":true}",
    )
    .unwrap();
    run(&["add", "LICENSE", "skills"]);
    run(&["commit", "-m", "fixture"]);
    service::execute(&root,json!({"action":"source-add","id":"local-demo","repository":format!("local:{}",upstream.display())})).unwrap();
    let registry_before: forge_core::model::Registry = forge_core::registry::load(&root).unwrap();
    let mut denied = registry_before.sources[0].clone();
    denied.roots = vec!["missing-root".into()];
    denied.excludes = vec!["LICENSE".into()];
    assert_eq!(
        forge_core::source::candidate(&root, &denied, false)
            .unwrap_err()
            .code,
        "license-review"
    );
    let cache = root.join(".forge-state/snapshots/local-demo");
    assert!(!fs::read_dir(&cache).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("stage-")
    }));
    let snapshot =
        service::execute(&root, json!({"action":"discover","source":"local-demo"})).unwrap();
    let files = snapshot["files"].as_array().unwrap();
    assert!(
        !files
            .iter()
            .any(|f| f["path"].as_str().unwrap().ends_with("Secrets.md"))
    );
    assert!(
        !files
            .iter()
            .any(|f| f["path"].as_str().unwrap().ends_with("auth.json"))
    );
    assert!(
        !files
            .iter()
            .any(|f| f["path"].as_str().unwrap().ends_with("settings.local.json"))
    );
    assert!(
        files
            .iter()
            .any(|f| f["path"] == "skills/hello/.codex/public.json")
    );
    let selection = root.join(format!(
        ".forge-state/snapshots/local-demo/{}.selection.json",
        snapshot["revision"].as_str().unwrap()
    ));
    fs::remove_file(&selection).unwrap();
    let migrated =
        service::execute(&root, json!({"action":"discover","source":"local-demo"})).unwrap();
    assert_eq!(migrated["content_digest"], snapshot["content_digest"]);
    assert!(selection.exists());
    service::execute(
        &root,
        json!({"action":"approve","source":"local-demo","digest":snapshot["content_digest"]}),
    )
    .unwrap();
    for id in ["hello-package", "second-package"] {
        service::execute(&root,json!({"action":"package-add","package":{"id":id,"name":id,"source":"local-demo","components":[],"targets":["claude-code"]}})).unwrap();
        let request = json!({"action":"build","package":id,"target":"claude-code"});
        let first = service::execute(&root, request.clone()).unwrap();
        let second = service::execute(&root, request).unwrap();
        assert_eq!(first["digest"], second["digest"]);
        assert_eq!(first["version"], second["version"]);
        assert!(
            root.join(format!("packages/claude-code/{id}/skills/hello/SKILL.md"))
                .exists()
        );
    }
    let lock: serde_json::Value = storage::read(&root.join("forge.lock.json")).unwrap();
    assert_eq!(lock["builds"].as_object().unwrap().len(), 2);
    let snapshot_path = root.join(format!(
        ".forge-state/snapshots/local-demo/{}",
        snapshot["revision"].as_str().unwrap()
    ));
    fs::remove_file(&selection).unwrap();
    let user_bytes = b"Preserve modified legacy cache";
    fs::write(snapshot_path.join("skills/hello/SKILL.md"), user_bytes).unwrap();
    assert_eq!(
        service::execute(&root, json!({"action":"discover","source":"local-demo"}))
            .unwrap_err()
            .code,
        "snapshot-selection"
    );
    assert_eq!(
        fs::read(snapshot_path.join("skills/hello/SKILL.md")).unwrap(),
        user_bytes
    );
    assert!(!selection.exists());
    fs::write(
        snapshot_path.join("skills/hello/SKILL.md"),
        b"---\nname: hello\ndescription: Harmless hello skill\n---\nSay hello.\n",
    )
    .unwrap();
    service::execute(&root, json!({"action":"discover","source":"local-demo"})).unwrap();
    service::execute(&root, json!({"action":"source-add","id":"dependency-source","repository":format!("local:{}", upstream.display())})).unwrap();
    service::execute(&root, json!({"action":"package-add","package":{"id":"dependency","name":"Dependency","source":"dependency-source","components":[],"targets":["claude-code"]}})).unwrap();
    service::execute(&root, json!({"action":"package-add","package":{"id":"dependent","name":"Dependent","source":"local-demo","components":[],"dependencies":["dependency"],"targets":["claude-code"]}})).unwrap();
    service::execute(&root, json!({"action":"source-policy","source":"local-demo","update":"approved-content-only","enabled":true})).unwrap();
    fs::write(
        upstream.join("skills/hello/SKILL.md"),
        b"---\nname: hello\ndescription: Harmless hello skill\n---\nSay a friendly hello.\n",
    )
    .unwrap();
    run(&["add", "skills/hello/SKILL.md"]);
    run(&["commit", "-m", "safe prompt fixture"]);
    let synced = service::execute(
        &root,
        json!({"action":"job-run","job":"safe-sync","request":{"action":"sync","apply":true}}),
    )
    .unwrap();
    assert_eq!(synced["status"], "partial");
    assert!(
        synced["result"]["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["package"] == "dependent" && row["code"] == "approval-required")
    );
    assert_eq!(
        fs::read(root.join("packages/claude-code/hello-package/skills/hello/SKILL.md")).unwrap(),
        b"---\nname: hello\ndescription: Harmless hello skill\n---\nSay a friendly hello.\n"
    );
    let dependent = service::execute(
        &root,
        json!({"action":"discover","source":"dependency-source"}),
    )
    .unwrap();
    service::execute(&root, json!({"action":"approve","source":"dependency-source","digest":dependent["content_digest"]})).unwrap();
    let retry = service::execute(&root, json!({"action":"job-retry","job":"safe-sync"})).unwrap();
    assert_eq!(retry["status"], "success");
    assert!(retry["result"]["sources"].as_array().unwrap().is_empty());
    assert!(
        root.join("packages/claude-code/dependent/skills/hello/SKILL.md")
            .exists()
    );
    fs::remove_dir_all(root).unwrap();
}
