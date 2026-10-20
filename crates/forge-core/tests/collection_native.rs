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
fn required_hooks_install_only_in_supported_scope_and_restore_user_configuration() {
    let f = Fixture(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(transaction::operation_id()),
    );
    let upstream = f.0.join("upstream");
    fs::create_dir_all(upstream.join("demo")).unwrap();
    fs::create_dir_all(upstream.join("hooks")).unwrap();
    let entry = b"---\nname: demo\ndescription: Required hook fixture\n---\nRequires its native companion.\n";
    let script = b"#!/bin/sh\nexit 0\n";
    let terms = b"MIT License\nPermission is hereby granted, free of charge\n";
    let config = storage::encoded(&json!({"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"bash \"$CLAUDE_PROJECT_DIR\"/.claude/hooks/demo.sh"}]}]}})).unwrap();
    fs::write(upstream.join("demo/SKILL.md"), entry).unwrap();
    fs::write(upstream.join("hooks/demo.sh"), script).unwrap();
    fs::write(upstream.join("hooks/demo.json"), &config).unwrap();
    fs::write(upstream.join("LICENSE"), terms).unwrap();
    let git = |args: &[&str]| {
        let mut command = vec!["-C".into(), upstream.to_string_lossy().into_owned()];
        command.extend(args.iter().map(|x| x.to_string()));
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
    let mut files = serde_json::Map::new();
    for (path, data, output) in [
        ("demo/SKILL.md", entry.as_slice(), "skills/demo/SKILL.md"),
        (
            "hooks/demo.sh",
            script.as_slice(),
            "native-originals/hooks/demo.sh",
        ),
        (
            "hooks/demo.json",
            config.as_slice(),
            "native-originals/hooks/demo.json",
        ),
        ("LICENSE", terms.as_slice(), "upstream-notices/LICENSE"),
    ] {
        files.insert(
            path.into(),
            json!({"digest":storage::digest(data),"mode":"100644","output":output}),
        );
    }
    let registration = json!({"schema_version":1,"collections":[{"id":"native","repository":format!("local:{}", upstream.display()),"branch":"main","reviewed_revision":git(&["rev-parse","HEAD"]).trim(),"skills":[{"id":"native-demo","root":"demo","license":"MIT","license_path":"LICENSE","license_digest":storage::digest(terms),"reviewed_distribution":{"version":1,"files":files,"native":{"files":{"hooks/demo.sh":".claude/hooks/demo.sh"},"hook_configs":["hooks/demo.json"]}}}]}]});
    storage::write(&f.0.join("registry/collections.json"), &registration).unwrap();
    let result = service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(result["status"], "success", "{result}");
    assert_eq!(
        service::execute(
            &f.0,
            json!({"action":"build","package":"native-demo","target":"codex"})
        )
        .unwrap_err()
        .code,
        "unsupported-required"
    );
    let destination = f.0.join("destination");
    fs::create_dir_all(destination.join(".claude")).unwrap();
    fs::write(
        destination.join(".claude/settings.json"),
        b"{\"unrelated\":true}\n",
    )
    .unwrap();
    assert_eq!(service::execute(&f.0, json!({"action":"install","package":"native-demo","target":"claude-code","scope":"user","destination":destination,"dry_run":true})).unwrap_err().code, "unsupported-required");
    let mut install = json!({"action":"install","package":"native-demo","target":"claude-code","scope":"project","destination":destination,"dry_run":false});
    assert_eq!(
        service::execute(&f.0, install.clone()).unwrap_err().code,
        "native-approval-required"
    );
    install["dry_run"] = json!(true);
    let preview = service::execute(&f.0, install.clone()).unwrap();
    install["native_approval"] = preview["native_approval"].clone();
    install["dry_run"] = json!(false);
    fs::write(destination.join(".claude/settings.json"), b"{\"hooks\":{\"PreToolUse\":[{\"matcher\":\"Bash\",\"hooks\":[{\"type\":\"command\",\"command\":\"preserved-user-guard\"}]}]}}\n").unwrap();
    assert_eq!(
        service::execute(&f.0, install.clone()).unwrap_err().code,
        "config-conflict"
    );
    fs::write(
        destination.join(".claude/settings.json"),
        b"{\"unrelated\":true}\n",
    )
    .unwrap();
    let receipt = service::execute(&f.0, install).unwrap();
    assert_eq!(
        fs::read(destination.join(".claude/hooks/demo.sh")).unwrap(),
        script
    );
    let config: Value = storage::read(&destination.join(".claude/settings.json")).unwrap();
    assert_eq!(config["unrelated"], true);
    assert_eq!(config["hooks"]["PreToolUse"][0]["matcher"], "Bash");
    let mut second_registration = registration;
    second_registration["collections"][0]["skills"][0]["id"] = json!("native-other");
    storage::write(&f.0.join("registry/collections.json"), &second_registration).unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap()["status"],
        "success"
    );
    // Restore both registrations for independent verification and installation.
    let mut both = second_registration;
    let mut previous_skill = both["collections"][0]["skills"][0].clone();
    previous_skill["id"] = json!("native-demo");
    both["collections"][0]["skills"]
        .as_array_mut()
        .unwrap()
        .push(previous_skill);
    storage::write(&f.0.join("registry/collections.json"), &both).unwrap();
    let mut second = json!({"action":"install","package":"native-other","target":"claude-code","scope":"project","destination":destination,"dry_run":true});
    let preview = service::execute(&f.0, second.clone()).unwrap();
    second["native_approval"] = preview["native_approval"].clone();
    second["dry_run"] = json!(false);
    let shared = service::execute(&f.0, second).unwrap();
    let updated = forge_core::install::update_installed(&f.0, &destination, false).unwrap();
    assert_eq!(updated["changes"], 0);
    assert_eq!(updated["receipts"].as_array().unwrap().len(), 2);
    service::execute(&f.0, json!({"action":"uninstall","receipt":shared["id"]})).unwrap();
    assert!(destination.join(".claude/hooks/demo.sh").exists());
    service::execute(&f.0, json!({"action":"uninstall","receipt":receipt["id"]})).unwrap();
    assert!(!destination.join(".claude/hooks/demo.sh").exists());
    let config: Value = storage::read(&destination.join(".claude/settings.json")).unwrap();
    assert_eq!(config, json!({"unrelated":true}));
}
