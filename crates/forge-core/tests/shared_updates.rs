use forge_core::{install, storage, transaction};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(format!("shared-update-{}", transaction::operation_id()));
        fs::create_dir_all(path.join("isolated project/.claude")).unwrap();
        Self(path)
    }
    fn destination(&self) -> PathBuf {
        self.0.join("isolated project")
    }
    fn package(&self, id: &str, bytes: &[u8], value: Value, mode: u32) {
        let package = self.0.join(format!("packages/claude-code/{id}"));
        fs::create_dir_all(package.join("skills/shared")).unwrap();
        fs::write(package.join("skills/shared/SKILL.md"), bytes).unwrap();
        storage::write(
            &package.join("provenance.json"),
            &json!({"file_modes":{"skills/shared/SKILL.md":mode}}),
        )
        .unwrap();
        storage::write(&package.join("compatibility.json"), &json!({"supported":true,"components":[{"kind":"settings","metadata":{"native_target":"claude-code","install_contribution":{"project_path":".claude/settings.json","user_path":".claude/settings.json","value":value}}}]})).unwrap();
    }
    fn install(&self, id: &str) -> Value {
        install::install(&self.0, id, "claude-code", &self.destination(), false).unwrap()
    }
    fn config(&self) -> PathBuf {
        self.destination().join(".claude/settings.json")
    }
    fn payload(&self) -> PathBuf {
        self.destination().join(".claude/skills/shared/SKILL.md")
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn coordinated_update_and_rollback_keep_shared_receipts_and_original_baseline_coherent() {
    let w = Workspace::new();
    storage::write(
        &w.config(),
        &json!({"agent":"original", "empty":{}, "user":1}),
    )
    .unwrap();
    for id in ["first", "second"] {
        w.package(
            id,
            b"First-party old bytes\n",
            json!({"agent":"old", "empty":{"managed":true}}),
            0o644,
        );
        w.install(id);
    }
    let mut current: Value = storage::read(&w.config()).unwrap();
    current["user"] = json!(2);
    current["extra"] = json!("user-owned");
    storage::write(&w.config(), &current).unwrap();
    let before = install::receipts(&w.0).unwrap();
    for id in ["first", "second"] {
        w.package(
            id,
            b"First-party new bytes\n",
            json!({"agent":"new", "empty":{"managed":true}}),
            0o755,
        );
    }
    let preview = install::update_installed(&w.0, &w.destination(), true).unwrap();
    assert_eq!(preview["dry_run"], true);
    assert_eq!(fs::read(w.payload()).unwrap(), b"First-party old bytes\n");
    let result = install::update_installed(&w.0, &w.destination(), false).unwrap();
    let operation = result["operation"].as_str().unwrap();
    let updated = install::receipts(&w.0).unwrap();
    assert_eq!(updated.len(), 2);
    assert!(updated.iter().all(|r| r.operation == operation));
    assert_eq!(updated[0].files, updated[1].files);
    assert_eq!(updated[0].modes, updated[1].modes);
    assert_eq!(fs::read(w.payload()).unwrap(), b"First-party new bytes\n");
    assert_eq!(
        storage::read::<Value>(&w.config()).unwrap(),
        json!({"agent":"new", "empty":{"managed":true}, "user":2, "extra":"user-owned"})
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(w.payload()).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
    transaction::rollback(&w.0, operation).unwrap();
    assert_eq!(fs::read(w.payload()).unwrap(), b"First-party old bytes\n");
    let restored = install::receipts(&w.0).unwrap();
    for r in &restored {
        assert_eq!(
            r.operation,
            before.iter().find(|p| p.id == r.id).unwrap().operation
        );
    }
    let result = install::update_installed(&w.0, &w.destination(), false).unwrap();
    for r in result["receipts"].as_array().unwrap() {
        install::uninstall(&w.0, r["id"].as_str().unwrap()).unwrap();
    }
    assert_eq!(
        storage::read::<Value>(&w.config()).unwrap(),
        json!({"agent":"original", "empty":{}, "user":2, "extra":"user-owned"})
    );
}

#[test]
fn coordinated_update_keeps_each_owners_fields_and_releases_dropped_shared_fields() {
    let w = Workspace::new();
    storage::write(&w.config(), &json!({"agent":"original", "unrelated":true})).unwrap();
    w.package("first", b"old", json!({"agent":"old", "first":1}), 0o644);
    w.package("second", b"old", json!({"agent":"old", "second":2}), 0o644);
    w.install("first");
    w.install("second");
    w.package("first", b"new", json!({"first":3}), 0o644);
    w.package("second", b"new", json!({"second":4}), 0o644);
    install::update_installed(&w.0, &w.destination(), false).unwrap();
    let owners = install::receipts(&w.0).unwrap();
    let first = owners.iter().find(|r| r.package == "first").unwrap();
    let second = owners.iter().find(|r| r.package == "second").unwrap();
    assert_eq!(
        first.configs[".claude/settings.json"]
            .fields
            .keys()
            .collect::<Vec<_>>(),
        vec!["/first"]
    );
    assert_eq!(
        second.configs[".claude/settings.json"]
            .fields
            .keys()
            .collect::<Vec<_>>(),
        vec!["/second"]
    );
    assert_eq!(
        storage::read::<Value>(&w.config()).unwrap(),
        json!({"agent":"original", "unrelated":true, "first":3, "second":4})
    );
    install::uninstall(&w.0, &first.id).unwrap();
    assert!(w.payload().exists());
    assert_eq!(
        storage::read::<Value>(&w.config()).unwrap(),
        json!({"agent":"original", "unrelated":true, "second":4})
    );
    install::uninstall(&w.0, &second.id).unwrap();
    assert_eq!(
        storage::read::<Value>(&w.config()).unwrap(),
        json!({"agent":"original", "unrelated":true})
    );
}

#[cfg(unix)]
#[test]
fn interrupted_mode_only_shared_update_accepts_already_compensated_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let w = Workspace::new();
    for id in ["first", "second"] {
        w.package(id, b"same", json!({"agent":"same"}), 0o644);
        w.install(id);
        w.package(id, b"same", json!({"agent":"same"}), 0o755);
    }
    let result = install::update_installed(&w.0, &w.destination(), false).unwrap();
    let journal_path = w.0.join(format!(
        ".forge-state/history/{}.json",
        result["operation"].as_str().unwrap()
    ));
    let mut journal: Value = storage::read(&journal_path).unwrap();
    journal["state"] = json!("rolling-back");
    storage::write(&journal_path, &journal).unwrap();
    fs::set_permissions(w.payload(), fs::Permissions::from_mode(0o644)).unwrap();
    transaction::recover(&w.0).unwrap();
    assert_eq!(
        fs::metadata(w.payload()).unwrap().permissions().mode() & 0o777,
        0o644
    );
    assert!(
        install::receipts(&w.0)
            .unwrap()
            .iter()
            .all(|r| r.modes[".claude/skills/shared/SKILL.md"] == 0o644)
    );
}

#[test]
fn shared_update_conflicts_are_preflighted_without_mutating_any_owner() {
    for scenario in [
        "next-bytes",
        "next-modes",
        "next-config",
        "disabled",
        "disabled-config",
        "user-payload",
        "user-config",
        "missing-package",
        "case-alias",
    ] {
        let w = Workspace::new();
        for id in ["first", "second"] {
            w.package(id, b"old", json!({"agent":"old"}), 0o644);
            w.install(id);
        }
        let owners = install::receipts(&w.0).unwrap();
        if scenario.starts_with("disabled") {
            install::disable(
                &w.0,
                &owners.iter().find(|r| r.package == "second").unwrap().id,
            )
            .unwrap();
        }
        for id in ["first", "second"] {
            w.package(id, b"new", json!({"agent":"new"}), 0o755);
        }
        match scenario {
            "next-bytes" => w.package("second", b"different", json!({"agent":"new"}), 0o755),
            "next-modes" => w.package("second", b"new", json!({"agent":"new"}), 0o644),
            "next-config" => w.package("second", b"new", json!({"agent":"different"}), 0o755),
            "disabled-config" => w.package("first", b"old", json!({"agent":"new"}), 0o644),
            "user-payload" => fs::write(w.payload(), b"user edit").unwrap(),
            "user-config" => storage::write(&w.config(), &json!({"agent":"user edit"})).unwrap(),
            "missing-package" => {
                fs::remove_dir_all(w.0.join("packages/claude-code/second")).unwrap()
            }
            "case-alias" => {
                let package = w.0.join("packages/claude-code/second");
                fs::rename(package.join("skills/shared"), package.join("skills/SHARED")).unwrap();
                storage::write(
                    &package.join("provenance.json"),
                    &json!({"file_modes":{"skills/SHARED/SKILL.md":493}}),
                )
                .unwrap();
            }
            _ => {}
        }
        let payload = fs::read(w.payload()).unwrap();
        let config = fs::read(w.config()).unwrap();
        let before = serde_json::to_value(install::receipts(&w.0).unwrap()).unwrap();
        let history = fs::read_dir(w.0.join(".forge-state/history"))
            .unwrap()
            .count();
        assert!(
            install::update_installed(&w.0, &w.destination(), false).is_err(),
            "{scenario}"
        );
        assert_eq!(fs::read(w.payload()).unwrap(), payload, "{scenario}");
        assert_eq!(fs::read(w.config()).unwrap(), config, "{scenario}");
        assert_eq!(
            serde_json::to_value(install::receipts(&w.0).unwrap()).unwrap(),
            before,
            "{scenario}"
        );
        assert_eq!(
            fs::read_dir(w.0.join(".forge-state/history"))
                .unwrap()
                .count(),
            history,
            "{scenario}"
        );
    }
}

#[test]
fn mapped_payload_collision_refuses_before_installation() {
    let w = Workspace::new();
    w.package("demo", b"skill", json!({"agent":"old"}), 0o644);
    let package = w.0.join("packages/claude-code/demo");
    fs::create_dir_all(package.join("skills/.forge-packages/demo")).unwrap();
    fs::write(package.join("LICENSE"), b"original notice").unwrap();
    fs::write(
        package.join("skills/.forge-packages/demo/LICENSE"),
        b"different companion",
    )
    .unwrap();
    assert_eq!(
        install::install(&w.0, "demo", "claude-code", &w.destination(), false)
            .unwrap_err()
            .code,
        "ownership-conflict"
    );
    assert!(!w.payload().exists());
    assert!(install::receipts(&w.0).unwrap().is_empty());
}

#[test]
fn equivalent_destination_spelling_updates_one_coherent_group() {
    let w = Workspace::new();
    for id in ["first", "second"] {
        w.package(id, b"old", json!({"agent":"old"}), 0o644);
        w.install(id);
    }
    let mut receipts = install::receipts(&w.0).unwrap();
    receipts[0].destination = w.destination().join(".");
    storage::write(
        &w.0.join(format!(".forge-state/receipts/{}.json", receipts[0].id)),
        &receipts[0],
    )
    .unwrap();
    let history = fs::read_dir(w.0.join(".forge-state/history"))
        .unwrap()
        .count();
    for id in ["first", "second"] {
        w.package(id, b"new", json!({"agent":"new"}), 0o644);
    }
    storage::write(
        &w.0.join("catalogs/collections.lock.json"),
        &json!({"schema_version":1,"entries":{}}),
    )
    .unwrap();
    storage::write(
        &w.0.join("catalogs/collections.json"),
        &json!({"schema_version":1,"plugins":[]}),
    )
    .unwrap();
    storage::write(
        &w.0.join(".claude-plugin/marketplace.json"),
        &json!({"name":"numera-skill-forge","owner":{"name":"KiaroSama"},"plugins":[]}),
    )
    .unwrap();
    let verified =
        forge_core::service::execute(&w.0, json!({"action":"collection-verify"})).unwrap();
    assert_eq!(verified["verified"], true);
    assert_eq!(verified["plugins"], 0);
    let response = forge_core::service::execute(&w.0, json!({"action":"update"})).unwrap();
    assert_eq!(response["status"], "success", "{response}");
    assert!(!w.0.join(".cursor-plugin/marketplace.json").exists());
    assert_eq!(response["installed"].as_array().unwrap().len(), 1);
    assert_eq!(
        fs::read_dir(w.0.join(".forge-state/history"))
            .unwrap()
            .count(),
        history + 1
    );
    assert_eq!(fs::read(w.payload()).unwrap(), b"new");
}

#[test]
fn multi_receipt_recovery_restores_every_owner_and_refuses_a_receipt_edit_before_compensation() {
    for interrupted in ["applying", "rolling-back"] {
        let w = Workspace::new();
        for id in ["first", "second"] {
            w.package(id, b"old", json!({"agent":"old"}), 0o644);
            w.install(id);
        }
        let before = install::receipts(&w.0).unwrap();
        for id in ["first", "second"] {
            w.package(id, b"new", json!({"agent":"new"}), 0o755);
        }
        let updated = install::update_installed(&w.0, &w.destination(), false).unwrap();
        let operation = updated["operation"].as_str().unwrap();
        let journal_path = w.0.join(format!(".forge-state/history/{operation}.json"));
        let mut journal: Value = storage::read(&journal_path).unwrap();
        assert_eq!(journal["receipts"].as_array().unwrap().len(), 2);
        journal["state"] = json!(interrupted);
        storage::write(&journal_path, &journal).unwrap();
        // Real persisted crash shape: one receipt compensated, another still applied.
        let first = &before[0];
        storage::write(
            &w.0.join(format!(".forge-state/receipts/{}.json", first.id)),
            first,
        )
        .unwrap();
        let edited_path =
            w.0.join(format!(".forge-state/receipts/{}.json", before[1].id));
        let applied = fs::read(&edited_path).unwrap();
        fs::write(&edited_path, b"user-edited receipt").unwrap();
        assert_eq!(
            transaction::recover(&w.0).unwrap_err().code,
            "recovery-conflict"
        );
        assert_eq!(fs::read(w.payload()).unwrap(), b"new");
        assert_eq!(fs::read(&edited_path).unwrap(), b"user-edited receipt");
        fs::write(&edited_path, applied).unwrap();
        transaction::recover(&w.0).unwrap();
        assert_eq!(fs::read(w.payload()).unwrap(), b"old");
        assert_eq!(
            storage::read::<Value>(&w.config()).unwrap(),
            json!({"agent":"old"})
        );
        let recovered = install::receipts(&w.0).unwrap();
        for owner in recovered {
            assert_eq!(
                owner.operation,
                before.iter().find(|r| r.id == owner.id).unwrap().operation
            );
        }
        assert_eq!(
            storage::read::<Value>(&journal_path).unwrap()["state"],
            "rolled-back"
        );
    }
}
