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
fn reviewed_design_changes_require_exact_acceptance_and_update_installed_content() {
    let f = Fixture(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(transaction::operation_id()),
    );
    let upstream = f.0.join("upstream");
    fs::create_dir_all(upstream.join("design-md/demo")).unwrap();
    let original = b"# Design\nUse readable spacing.\n";
    fs::write(upstream.join("design-md/demo/DESIGN.md"), original).unwrap();
    fs::write(
        upstream.join("design-md/demo/README.md"),
        b"Old companion\n",
    )
    .unwrap();
    let license = b"MIT License\nPermission is hereby granted, free of charge\n";
    fs::write(upstream.join("LICENSE"), license).unwrap();
    fs::write(upstream.join("README.md"), b"Original rights disclaimer\n").unwrap();
    let git = |args: &[&str]| {
        let mut a = vec!["-C".into(), upstream.to_string_lossy().into_owned()];
        a.extend(args.iter().map(|s| s.to_string()));
        String::from_utf8(process::git(&a, 15).unwrap()).unwrap()
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
    let mut registry = json!({"schema_version":1,"collections":[{"id":"design","repository":format!("local:{}",upstream.display()),"branch":"main","reviewed_revision":git(&["rev-parse","HEAD"]).trim(),"skills":[{"id":"skill-design-demo","root":"design-md/demo","license":"MIT","license_path":"LICENSE","license_digest":storage::digest(license),"reviewed_design":{"name":"design-demo","source_digest":storage::digest(original),"adapter_version":1}}]}]});
    let disclaimer = b"Original rights disclaimer\n";
    let notice_path = format!("registry/licenses/{}.txt", storage::digest(disclaimer));
    fs::create_dir_all(f.0.join("registry/licenses")).unwrap();
    fs::write(f.0.join(&notice_path), disclaimer).unwrap();
    registry["collections"][0]["skills"][0]["notices"] = json!([{"path":notice_path,"digest":storage::digest(disclaimer),"source":format!("https://github.com/fixture/design/blob/{}/README.md",registry["collections"][0]["reviewed_revision"].as_str().unwrap()),"license":"MIT"}]);
    storage::write(&f.0.join("registry/collections.json"), &registry).unwrap();
    let run = |r| service::execute(&f.0, r).unwrap();
    assert_eq!(
        run(json!({"action":"collection-sync","apply":true}))["status"],
        "success"
    );
    let destination = f.0.join("consumer");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("unrelated.txt"), b"user data").unwrap();
    run(
        json!({"action":"install","package":"skill-design-demo","target":"claude-code","destination":destination,"dry_run":false}),
    );
    let installed = destination.join(".claude/skills/design-demo/DESIGN.md");
    let before = fs::read(f.0.join("catalogs/collections.lock.json")).unwrap();
    fs::write(
        upstream.join("design-md/demo/README.md"),
        b"Changed companion only\n",
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
        "companion",
    ]);
    let held = run(json!({"action":"collection-sync","apply":true}));
    assert_eq!(held["skills"][0]["status"], "review-required");
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        before
    );
    let changed = b"# Design\nUse accessible contrast.\n";
    fs::write(upstream.join("design-md/demo/DESIGN.md"), changed).unwrap();
    fs::remove_file(upstream.join("design-md/demo/README.md")).unwrap();
    fs::write(upstream.join("README.md"), b"Updated rights disclaimer\n").unwrap();
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
    let review = run(
        json!({"action":"collection-design-review","collection":"design","root":"design-md/demo"}),
    );
    assert_eq!(review["status"], "review-required");
    assert!(
        review["diff"]
            .as_str()
            .unwrap()
            .contains("accessible contrast")
    );
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        before
    );
    let accept = json!({"action":"collection-design-accept","collection":"design","root":"design-md/demo","revision":review["revision"],"digest":review["digest"]});
    let mut wrong = accept.clone();
    wrong["digest"] = json!("0".repeat(64));
    assert!(service::execute(&f.0, wrong).is_err());
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        before
    );
    assert_eq!(run(accept.clone())["status"], "updated");
    let package =
        f.0.join("packages/claude-code/skill-design-demo/skills/design-demo");
    assert_eq!(fs::read(package.join("DESIGN.md")).unwrap(), changed);
    assert!(!package.join("README.md").exists());
    let lock: Value = storage::read(&f.0.join("catalogs/collections.lock.json")).unwrap();
    assert_eq!(
        lock["entries"]["design/skill-design-demo"]["version"],
        "1.0.1"
    );
    assert_eq!(run(accept.clone())["status"], "unchanged");
    let receipt_path = f.0.join(format!(
        ".forge-state/collections/design-approvals/{}.json",
        review["digest"].as_str().unwrap()
    ));
    let receipt = fs::read(&receipt_path).unwrap();
    let mut tampered: Value = serde_json::from_slice(&receipt).unwrap();
    tampered["revision"] = json!("1".repeat(40));
    storage::write(&receipt_path, &tampered).unwrap();
    let mut replay = accept;
    replay["revision"] = tampered["revision"].clone();
    assert!(service::execute(&f.0, replay).is_err());
    fs::write(receipt_path, receipt).unwrap();
    run(json!({"action":"collection-verify"}));
    run(json!({"action":"update"}));
    assert_eq!(fs::read(&installed).unwrap(), changed);
    assert_eq!(
        fs::read(destination.join("unrelated.txt")).unwrap(),
        b"user data"
    );
    let stable = fs::read(f.0.join("catalogs/collections.lock.json")).unwrap();
    let same = run(
        json!({"action":"collection-design-review","collection":"design","root":"design-md/demo"}),
    );
    assert_eq!(same["status"], "unchanged");
    run(
        json!({"action":"collection-design-accept","collection":"design","root":"design-md/demo","revision":same["revision"],"digest":same["digest"]}),
    );
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        stable
    );
    fs::create_dir_all(upstream.join("design-md/new.app")).unwrap();
    fs::write(
        upstream.join("design-md/new.app/DESIGN.md"),
        b"# New design\n",
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
        "new design",
    ]);
    let inventory = run(json!({"action":"collection-design-list","collection":"design"}));
    assert!(
        inventory["designs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["root"] == "design-md/new.app" && v["status"] == "new")
    );
    let new = run(
        json!({"action":"collection-design-review","collection":"design","root":"design-md/new.app"}),
    );
    let new_accept = json!({"action":"collection-design-accept","collection":"design","root":"design-md/new.app","revision":new["revision"],"digest":new["digest"]});
    let registration = f.0.join("registry/collections.json");
    let raw = fs::read(&registration).unwrap();
    let mut value: Value = serde_json::from_slice(&raw).unwrap();
    value["collections"][0]["skills"][0]["attribution"] = json!("Concurrent change");
    storage::write(&registration, &value).unwrap();
    assert!(service::execute(&f.0, new_accept.clone()).is_err());
    fs::write(&registration, &raw).unwrap();
    assert_eq!(run(new_accept)["status"], "updated");
    assert!(
        f.0.join("packages/claude-code/skill-design-new-app/skills/design-new-app/SKILL.md")
            .exists()
    );
    fs::write(
        upstream.join("design-md/demo/DESIGN.md"),
        b"# Third revision\n",
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
        "third",
    ]);
    let third = run(
        json!({"action":"collection-design-review","collection":"design","root":"design-md/demo"}),
    );
    let third_accept = json!({"action":"collection-design-accept","collection":"design","root":"design-md/demo","revision":third["revision"],"digest":third["digest"]});
    let entry = fs::read(package.join("SKILL.md")).unwrap();
    fs::write(package.join("SKILL.md"), b"user package edit").unwrap();
    assert!(service::execute(&f.0, third_accept.clone()).is_err());
    fs::write(package.join("SKILL.md"), entry).unwrap();
    run(third_accept);
    fs::write(&installed, b"user installed edit").unwrap();
    let update = service::execute(&f.0, json!({"action":"update"}));
    assert!(update.is_err() || update.unwrap()["status"] != "success");
    assert_eq!(fs::read(&installed).unwrap(), b"user installed edit");
    git(&["rm", "design-md/demo/DESIGN.md"]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-m",
        "remove source",
    ]);
    let inventory = run(json!({"action":"collection-design-list","collection":"design"}));
    assert!(
        inventory["designs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["root"] == "design-md/demo" && v["status"] == "missing")
    );
    assert!(service::execute(&f.0,json!({"action":"collection-design-review","collection":"design","root":"design-md/demo"})).is_err());
    assert_eq!(
        fs::read(package.join("DESIGN.md")).unwrap(),
        b"# Third revision\n"
    );
}
