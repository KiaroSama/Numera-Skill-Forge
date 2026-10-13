use forge_core::{process, service, storage};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(format!(
                "delivery-{}-{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap()
            ));
        let upstream = root.join("upstream");
        fs::create_dir_all(upstream.join("skills/demo/references")).unwrap();
        fs::write(
            upstream.join("skills/demo/SKILL.md"),
            b"---\nname: demo\ndescription: Original fixture\n---\nOriginal sentence.\n",
        )
        .unwrap();
        let license = b"MIT License\nPermission is hereby granted, free of charge\n";
        fs::write(upstream.join("LICENSE"), license).unwrap();
        fs::write(
            upstream.join("skills/demo/references/readme.md"),
            b"Original companion\n",
        )
        .unwrap();
        let f = Self(root);
        f.git(&["init", "-b", "main"]);
        f.commit();
        storage::write(
            &f.0.join("registry/collections.json"),
            &json!({"schema_version":1,"collections":[{
            "id":"fixture","repository":format!("local:{}",upstream.display()),"branch":"main",
            "reviewed_revision":f.git(&["rev-parse","HEAD"]).trim(),"skills":[{
                "id":"fixture-demo","root":"skills/demo","license":"MIT","license_path":"LICENSE",
                "license_digest":storage::digest(license)}]}]}),
        )
        .unwrap();
        f.request(json!({"action":"collection-sync","apply":true}));
        f
    }
    fn git(&self, args: &[&str]) -> String {
        let mut command = vec![
            "-C".into(),
            self.0.join("upstream").to_string_lossy().into_owned(),
        ];
        command.extend(args.iter().map(|s| s.to_string()));
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
    fn request(&self, request: Value) -> Value {
        service::execute(&self.0, request).unwrap()
    }
}

#[test]
fn accepted_original_skills_install_at_every_verified_host_scope_with_lifecycle() {
    let f = Fixture::new();
    let matrix = [
        ("claude-code", ".claude/skills", ".claude/skills"),
        ("codex", ".agents/skills", ".agents/skills"),
        ("cursor", ".cursor/skills", ".cursor/skills"),
        ("hermes", ".hermes/skills", ".hermes/skills"),
        ("antigravity", ".agents/skills", ".gemini/config/skills"),
        ("opencode", ".opencode/skills", ".config/opencode/skills"),
        ("opencode-v1", ".opencode/skills", ".config/opencode/skills"),
        ("opencode-v2", ".opencode/skills", ".config/opencode/skills"),
        ("copilot", ".github/skills", ".copilot/skills"),
        ("vscode", ".github/skills", ".copilot/skills"),
        ("gemini", ".gemini/skills", ".gemini/skills"),
        ("amp", ".agents/skills", ".config/agents/skills"),
        ("pi", ".pi/skills", ".pi/agent/skills"),
        ("kiro", ".kiro/skills", ".kiro/skills"),
        ("windsurf", ".devin/skills", ".config/devin/skills"),
        ("cline", ".cline/skills", ".cline/skills"),
        ("kilo", ".kilo/skills", ".kilo/skills"),
        ("continue", ".continue/skills", ".continue/skills"),
        ("openhands", ".agents/skills", ".agents/skills"),
        ("openclaw", "skills", ".openclaw/skills"),
    ];
    let original =
        fs::read(f.0.join("packages/claude-code/fixture-demo/skills/demo/SKILL.md")).unwrap();
    assert_eq!(
        f.request(json!({"action":"catalog"}))[0]["id"],
        "fixture-demo"
    );
    for (target, project, user) in matrix {
        assert_eq!(
            f.request(json!({"action":"compatibility","package":"fixture-demo","target":target}))["supported"],
            true
        );
        f.request(json!({"action":"build","package":"fixture-demo","target":target}));
        for (scope, prefix) in [("project", project), ("user", user)] {
            let destination = f.0.join(format!("destination-{target}-{scope}"));
            fs::create_dir(&destination).unwrap();
            fs::write(destination.join("unrelated.txt"), b"user content").unwrap();
            let request = json!({"action":"install","package":"fixture-demo","target":target,"scope":scope,"destination":destination,"dry_run":true});
            let preview = f.request(request.clone());
            assert!(
                preview["files"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(format!("{prefix}/demo/SKILL.md")))
            );
            assert!(!destination.join(prefix).exists());
            let mut apply = request;
            apply["dry_run"] = json!(false);
            let installed = f.request(apply.clone());
            let id = installed["id"].as_str().unwrap();
            let entry = destination.join(prefix).join("demo/SKILL.md");
            assert_eq!(fs::read(&entry).unwrap(), original);
            assert_eq!(
                fs::read(destination.join(prefix).join("demo/references/readme.md")).unwrap(),
                b"Original companion\n"
            );
            assert!(!installed["files"].as_object().unwrap().keys().any(|p| {
                p.contains(".codex-plugin")
                    || p.contains(".cursor-plugin")
                    || p.ends_with("plugin.json")
            }));
            f.request(json!({"action":"disable","receipt":id}));
            assert!(!entry.exists());
            f.request(json!({"action":"enable","receipt":id}));
            assert_eq!(fs::read(&entry).unwrap(), original);
            apply["dry_run"] = json!(true);
            assert_eq!(f.request(apply)["changes"], 0);
            fs::write(&entry, b"user edit").unwrap();
            assert_eq!(
                service::execute(&f.0, json!({"action":"uninstall","receipt":id}))
                    .unwrap_err()
                    .code,
                "conflict"
            );
            assert_eq!(fs::read(&entry).unwrap(), b"user edit");
            fs::write(&entry, &original).unwrap();
            let removed = f.request(json!({"action":"uninstall","receipt":id}));
            assert!(!entry.exists());
            if target == "hermes" && scope == "project" {
                f.request(json!({"action":"rollback","operation":removed["operation"]}));
                assert_eq!(fs::read(&entry).unwrap(), original);
                f.request(json!({"action":"uninstall","receipt":id}));
            }
            assert_eq!(
                fs::read(destination.join("unrelated.txt")).unwrap(),
                b"user content"
            );
        }
    }
    for target in ["hermes-native", "amp-native", "openclaw-native"] {
        assert_eq!(
            service::execute(
                &f.0,
                json!({"action":"build","package":"fixture-demo","target":target})
            )
            .unwrap_err()
            .code,
            "native-install-required"
        );
    }
}

#[test]
fn distinct_same_name_packages_refuse_even_when_the_main_file_is_identical() {
    let f = Fixture::new();
    let path = f.0.join("registry/collections.json");
    let mut registry: Value = storage::read(&path).unwrap();
    let mut second = registry["collections"][0]["skills"][0].clone();
    second["id"] = json!("fixture-second");
    registry["collections"][0]["skills"]
        .as_array_mut()
        .unwrap()
        .push(second);
    storage::write(&path, &registry).unwrap();
    f.request(json!({"action":"collection-sync","apply":true}));
    let destination = f.0.join("selected-project");
    fs::create_dir(&destination).unwrap();
    f.request(json!({"action":"install","package":"fixture-demo","target":"opencode-v2","destination":destination,"dry_run":false}));
    let error=service::execute(&f.0,json!({"action":"install","package":"fixture-second","target":"opencode-v2","destination":destination,"dry_run":false})).unwrap_err();
    assert_eq!(error.code, "skill-name-conflict");
    assert_eq!(service::execute(&f.0,json!({"action":"install","package":"fixture-second","target":"pi","destination":destination,"dry_run":false})).unwrap_err().code,"skill-name-conflict");
    assert_eq!(
        f.request(json!({"action":"installed"}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    fs::write(
        f.0.join("upstream/skills/demo/SKILL.md"),
        b"---\nname: demo\ndescription: Original fixture\n---\nImproved sentence.\n",
    )
    .unwrap();
    f.commit();
    f.request(json!({"action":"collection-sync","apply":true}));
    assert_eq!(
        f.request(json!({"action":"update","installed":true}))["status"],
        "success"
    );
    assert!(
        String::from_utf8(fs::read(destination.join(".opencode/skills/demo/SKILL.md")).unwrap())
            .unwrap()
            .contains("Improved sentence.")
    );
    let mut skill =
        String::from_utf8(fs::read(f.0.join("upstream/skills/demo/SKILL.md")).unwrap()).unwrap();
    skill = skill.replace("Original fixture", &"x".repeat(1025));
    fs::write(f.0.join("upstream/skills/demo/SKILL.md"), skill).unwrap();
    f.commit();
    assert_eq!(
        f.request(json!({"action":"collection-sync","apply":true}))["skills"][0]["status"],
        "review-required"
    );
    let collision = f.0.join("registry/sources.json");
    storage::write(&collision,&json!({"schema_version":1,"sources":[{"id":"generic","repository":"https://github.com/example/fixture"}],
        "packages":[{"id":"fixture-demo","name":"Generic same ID","source":"generic","components":[]}]})).unwrap();
    for action in ["build", "compatibility", "package-inspect", "install"] {
        assert_eq!(service::execute(&f.0,json!({"action":action,"package":"fixture-demo","target":"pi","destination":destination})).unwrap_err().code,"package-identity-conflict");
    }
}
