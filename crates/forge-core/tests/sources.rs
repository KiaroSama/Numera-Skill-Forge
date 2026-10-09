use forge_core::{service, storage};
use serde_json::json;
use std::{fs, path::PathBuf, process::Command};

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
        let result = Command::new("git")
            .arg("-C")
            .arg(&upstream)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .output()
            .unwrap();
        assert!(result.status.success(), "git fixture failed");
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
    run(&["add", "LICENSE", "skills"]);
    run(&["commit", "-m", "fixture"]);
    service::execute(&root,json!({"action":"source-add","id":"local-demo","repository":format!("local:{}",upstream.display())})).unwrap();
    let snapshot =
        service::execute(&root, json!({"action":"discover","source":"local-demo"})).unwrap();
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
    fs::remove_dir_all(root).unwrap();
}
