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
fn reviewed_bundle_preserves_native_siblings_and_selected_identity() {
    let f = Fixture(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(transaction::operation_id()),
    );
    let upstream = f.0.join("upstream");
    let originals: [(&str, &[u8], &str); 5] = [
        ("bundle/z-router/SKILL.md", b"---\nname: z-router\ndescription: Routes to the original a-child sibling\n---\nRead ../a-child/SKILL.md.\n", "skills/z-router/SKILL.md"),
        ("bundle/a-child/SKILL.md", b"---\nname: a-child\ndescription: Original downstream child\n---\nRead references/literal.md.\n", "skills/a-child/SKILL.md"),
        ("bundle/a-child/references/literal.md", b"# Literal\nOriginal child companion.\n", "skills/a-child/references/literal.md"),
        ("bundle/NOTICE.md", b"Original complete bundle notice.\n", "upstream-notices/bundle-NOTICE.md"),
        ("LICENSE", b"MIT License\nPermission is hereby granted, free of charge\n", "upstream-notices/LICENSE"),
    ];
    let mut files = serde_json::Map::new();
    for (path, bytes, output) in originals {
        let destination = upstream.join(path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, bytes).unwrap();
        files.insert(
            path.into(),
            json!({"digest":storage::digest(bytes),"mode":"100644","output":output}),
        );
    }
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
    let registry = f.0.join("registry/collections.json");
    let mut registration = json!({"schema_version":1,"collections":[{"id":"siblings","repository":format!("local:{}",upstream.display()),"branch":"main","reviewed_revision":git(&["rev-parse","HEAD"]).trim(),"skills":[{"id":"sibling-bundle","root":"bundle/z-router","license":"MIT","license_path":"LICENSE","license_digest":storage::digest(originals[4].1),"reviewed_distribution":{"version":1,"files":files}}]}]});
    storage::write(&registry, &registration).unwrap();
    let result = service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(result["status"], "success", "{result}");
    let catalog = service::execute(&f.0, json!({"action":"catalog"})).unwrap();
    assert_eq!(catalog[0]["name"], "z-router", "{catalog}");
    let package = f.0.join("packages/claude-code/sibling-bundle");
    for (_, bytes, output) in originals {
        assert_eq!(fs::read(package.join(output)).unwrap(), bytes);
    }
    service::execute(&f.0, json!({"action":"collection-verify"})).unwrap();
    let accepted = fs::read(f.0.join("catalogs/collections.lock.json")).unwrap();
    service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        accepted
    );
    fs::remove_dir_all(&package).unwrap();
    service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(
        fs::read(package.join("skills/a-child/references/literal.md")).unwrap(),
        originals[2].1
    );
    let consumer = f.0.join("consumer");
    fs::create_dir(&consumer).unwrap();
    let request = json!({"action":"install","package":"sibling-bundle","target":"claude-code","destination":consumer,"dry_run":false});
    let installed = service::execute(&f.0, request.clone()).unwrap();
    let receipt = installed["id"].as_str().unwrap();
    assert_eq!(
        fs::read(consumer.join(".claude/skills/a-child/references/literal.md")).unwrap(),
        originals[2].1
    );
    let mut preview = request;
    preview["dry_run"] = json!(true);
    assert_eq!(service::execute(&f.0, preview).unwrap()["changes"], 0);
    service::execute(&f.0, json!({"action":"disable","receipt":receipt})).unwrap();
    assert!(!consumer.join(".claude/skills/a-child/SKILL.md").exists());
    service::execute(&f.0, json!({"action":"enable","receipt":receipt})).unwrap();
    service::execute(&f.0, json!({"action":"uninstall","receipt":receipt})).unwrap();
    assert!(!consumer.join(".claude/skills/z-router/SKILL.md").exists());
    assert!(!consumer.join(".claude/skills/a-child/SKILL.md").exists());
    let approved = registration.clone();
    registration["collections"][0]["skills"][0]["reviewed_distribution"]["files"]
        .as_object_mut()
        .unwrap()
        .remove("bundle/a-child/SKILL.md");
    storage::write(&registry, &registration).unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-verify"}))
            .unwrap_err()
            .code,
        "approval-stale"
    );
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        accepted
    );
    registration = approved.clone();
    let child = format!(
        "---\nname: a-child\ndescription: {}\n---\nOriginal downstream child.\n",
        "x".repeat(1025)
    );
    fs::write(upstream.join("bundle/a-child/SKILL.md"), &child).unwrap();
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-m",
        "child metadata",
    ]);
    registration["collections"][0]["skills"][0]["id"] = json!("long-child-bundle");
    registration["collections"][0]["skills"][0]["reviewed_distribution"]["files"]["bundle/a-child/SKILL.md"]
        ["digest"] = json!(storage::digest(child.as_bytes()));
    registration["collections"][0]["reviewed_revision"] = json!(git(&["rev-parse", "HEAD"]).trim());
    storage::write(&registry, &registration).unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap()["status"],
        "success"
    );
    assert_eq!(
        service::execute(
            &f.0,
            json!({"action":"build","package":"long-child-bundle","target":"codex"})
        )
        .unwrap_err()
        .code,
        "skill-metadata"
    );
    registration = approved;
    registration["collections"][0]["skills"][0]["reviewed_distribution"]["files"][".ai/private.md"] =
        json!({"digest":"0".repeat(64),"mode":"100644","output":"skills/z-router/private.md"});
    storage::write(&registry, &registration).unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-sync","apply":true}))
            .unwrap_err()
            .code,
        "distribution-review"
    );
}

#[test]
fn reviewed_layout_preserves_bytes_and_refuses_withdrawal() {
    let f = Fixture(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(transaction::operation_id()),
    );
    let upstream = f.0.join("upstream");
    fs::create_dir_all(upstream.join("demo/refrences")).unwrap();
    let original = b"---\nname: demo\ndescription: Reference layout fixture\n---\nRead references/keywords.md.\n";
    let companion = b"# Keywords\nLiteral original companion.\n";
    let license = b"MIT License\nPermission is hereby granted, free of charge\n";
    fs::write(upstream.join("demo/SKILL.md"), original).unwrap();
    fs::write(upstream.join("demo/refrences/keywords.md"), companion).unwrap();
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
    let review = json!({"version":1,"files":{
        "demo/SKILL.md":{"digest":storage::digest(original),"mode":"100644","output":"skills/demo/SKILL.md"},
        "demo/refrences/keywords.md":{"digest":storage::digest(companion),"mode":"100644","output":"skills/demo/references/keywords.md"},
        "LICENSE":{"digest":storage::digest(license),"mode":"100644","output":"upstream-notices/LICENSE"}
    }});
    let mut review = review;
    let extra = b"Apache License\nVersion 2.0\n";
    fs::create_dir_all(f.0.join("registry/licenses")).unwrap();
    fs::write(f.0.join("registry/licenses/extra.txt"), extra).unwrap();
    review["supplements"] = json!({"registry/licenses/extra.txt":{"digest":storage::digest(extra),"output":"skills/demo/references/LICENSE.txt","source":"https://github.com/example/original/blob/0123456789012345678901234567890123456789/LICENSE","license":"Apache-2.0"}});
    let mut registration = json!({"schema_version":1,"collections":[{"id":"layout","repository":format!("local:{}",upstream.display()),"branch":"main","reviewed_revision":git(&["rev-parse","HEAD"]).trim(),"skills":[{"id":"layout-demo","root":"demo","license":"MIT","license_path":"LICENSE","license_digest":storage::digest(license),"reviewed_distribution":review}]}]});
    let registry = f.0.join("registry/collections.json");
    storage::write(&registry, &registration).unwrap();
    let result = service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(result["status"], "success", "{result}");
    let package = f.0.join("packages/claude-code/layout-demo");
    assert_eq!(
        fs::read(package.join("skills/demo/references/keywords.md")).unwrap(),
        companion
    );
    assert_eq!(
        fs::read(package.join("skills/demo/SKILL.md")).unwrap(),
        original
    );
    assert!(!package.join("skills/demo/refrences").exists());
    assert_eq!(
        fs::read(package.join("skills/demo/references/LICENSE.txt")).unwrap(),
        extra
    );
    service::execute(&f.0, json!({"action":"collection-verify"})).unwrap();
    let accepted = fs::read(f.0.join("catalogs/collections.lock.json")).unwrap();
    service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        accepted
    );
    let approved = registration.clone();
    for (field, value) in [("version", json!(2)), ("files", json!({}))] {
        registration = approved.clone();
        registration["collections"][0]["skills"][0]["reviewed_distribution"][field] = value;
        storage::write(&registry, &registration).unwrap();
        assert_eq!(
            service::execute(&f.0, json!({"action":"collection-sync","apply":true}))
                .unwrap_err()
                .code,
            "distribution-review"
        );
        assert_eq!(
            fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
            accepted
        );
    }
    registration = approved.clone();
    registration["collections"][0]["skills"][0]["reviewed_distribution"]["files"]["demo/refrences/keywords.md"]
        ["output"] = json!("skills/demo/SKILL.md");
    storage::write(&registry, &registration).unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-sync","apply":true}))
            .unwrap_err()
            .code,
        "distribution-review"
    );
    storage::write(&registry, &approved).unwrap();
    fs::remove_dir_all(&package).unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap()["status"],
        "success"
    );
    assert_eq!(
        fs::read(package.join("skills/demo/references/keywords.md")).unwrap(),
        companion
    );
    fs::write(
        upstream.join("demo/refrences/keywords.md"),
        b"Changed source\n",
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
    let held = service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(held["status"], "partial");
    assert_eq!(held["skills"][0]["code"], "distribution-review");
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        accepted
    );
    registration = approved.clone();
    registration["collections"][0]["skills"][0]["reviewed_distribution"]["obligations"] = json!([{"license":"GPL-3.0-only","files":["demo/SKILL.md"],"notices":["missing-notice"],"attribution":"Original fixture authors; unchanged source","source":"https://github.com/example/original/blob/0123456789012345678901234567890123456789/demo/SKILL.md"}]);
    storage::write(&registry, &registration).unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-sync","apply":true}))
            .unwrap_err()
            .code,
        "distribution-review"
    );
    registration = approved.clone();
    registration["collections"][0]["skills"][0]["reviewed_distribution"]["obligations"] = json!([{"license":"MIT","files":["demo/SKILL.md","demo/refrences/keywords.md","LICENSE"],"notices":["LICENSE"],"attribution":"Original fixture authors; unchanged source","source":"https://github.com/example/original/blob/0123456789012345678901234567890123456789/demo/SKILL.md"}]);
    storage::write(&registry, &registration).unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-verify"}))
            .unwrap_err()
            .code,
        "approval-stale"
    );
    registration = approved;
    registration["collections"][0]["skills"][0]
        .as_object_mut()
        .unwrap()
        .remove("reviewed_distribution");
    storage::write(&registry, &registration).unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-verify"}))
            .unwrap_err()
            .code,
        "approval-stale"
    );
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        accepted
    );
    // GPL's section 6 says "noncommercially" about one optional source-offer route,
    // not a prohibition on redistributing the selected unchanged source.
    let gpl = include_bytes!("../../../LICENSE");
    fs::write(upstream.join("LICENSE"), gpl).unwrap();
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-m",
        "GPL fixture",
    ]);
    let mut licensed = registration;
    let skill = &mut licensed["collections"][0]["skills"][0];
    skill["id"] = json!("gpl-demo");
    skill["license"] = json!("GPL-3.0-only");
    skill["license_digest"] = json!(storage::digest(gpl));
    review["files"]["LICENSE"]["digest"] = json!(storage::digest(gpl));
    review["files"]["demo/refrences/keywords.md"]["digest"] =
        json!(storage::digest(b"Changed source\n"));
    review["obligations"] = json!([{"license":"GPL-3.0-only","files":["demo/SKILL.md","demo/refrences/keywords.md","LICENSE"],"notices":["LICENSE"],"attribution":"Original fixture source, unchanged; GNU GPL version 3","source":"https://github.com/example/original/blob/0123456789012345678901234567890123456789/demo/SKILL.md"}]);
    review["files"]["demo/SKILL.md"]["change_notice"] = json!(
        "Forge distribution notice: 2026-10-20. Original fixture source retained; append this prominent change notice only."
    );
    review["supplements"]["registry/licenses/extra.txt"]["source"] =
        json!("https://registry.npmjs.org/android-cicd/-/android-cicd-1.0.4.tgz");
    review["supplements"]["registry/licenses/extra.txt"]["artifact"] = json!({"digest":"0".repeat(64),"member":"package/templates/gradle/signing.gradle","integrity":format!("sha512-{}", "A".repeat(88))});
    skill["reviewed_distribution"] = review;
    let mut collision = licensed.clone();
    collision["collections"][0]["skills"][0]["reviewed_distribution"]["files"]["demo/refrences/keywords.md"]
        ["output"] = json!("upstream-notices/original-files/demo/SKILL.md");
    storage::write(&registry, &collision).unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-sync","apply":true}))
            .unwrap_err()
            .code,
        "distribution-review"
    );
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.lock.json")).unwrap(),
        accepted
    );
    licensed["collections"][0]["reviewed_revision"] = json!(git(&["rev-parse", "HEAD"]).trim());
    storage::write(&registry, &licensed).unwrap();
    let result = service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    assert_eq!(result["status"], "success", "{result}");
    assert_eq!(
        fs::read(f.0.join("packages/claude-code/gpl-demo/upstream-notices/LICENSE")).unwrap(),
        gpl
    );
    assert_eq!(
        fs::read(
            f.0.join("packages/claude-code/gpl-demo/upstream-notices/original-files/demo/SKILL.md")
        )
        .unwrap(),
        original
    );
    let emitted =
        fs::read_to_string(f.0.join("packages/claude-code/gpl-demo/skills/demo/SKILL.md")).unwrap();
    assert!(emitted.starts_with(std::str::from_utf8(original).unwrap()));
    assert!(emitted.ends_with("append this prominent change notice only.\n"));
    service::execute(
        &f.0,
        json!({"action":"build","package":"gpl-demo","target":"claude-code"}),
    )
    .unwrap();
}
