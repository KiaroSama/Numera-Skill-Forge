use forge_core::{install, storage, transaction};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(format!("config-{}", transaction::operation_id()));
        fs::create_dir_all(path.join("project/.claude")).unwrap();
        Self(path)
    }
    fn package(&self, id: &str, value: Value) {
        let package = self.0.join(format!("packages/claude-code/{id}"));
        fs::create_dir_all(&package).unwrap();
        storage::write(&package.join("compatibility.json"), &json!({"supported":true,"components":[{"kind":"settings","metadata":{"native_target":"claude-code","install_contribution":{"project_path":".claude/settings.json","user_path":".claude/settings.json","value":value}}}]})).unwrap();
    }
    fn install(&self, id: &str) -> Value {
        install::install(&self.0, id, "claude-code", &self.0.join("project"), false).unwrap()
    }
    fn config(&self) -> PathBuf {
        self.0.join("project/.claude/settings.json")
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn reenabled_shared_owner_adopts_new_user_baseline_for_final_release() {
    let workspace = Workspace::new();
    storage::write(&workspace.config(), &json!({"agent":"original"})).unwrap();
    workspace.package("first", json!({"agent":"managed"}));
    let first = workspace.install("first");
    install::disable(&workspace.0, first["id"].as_str().unwrap()).unwrap();
    storage::write(&workspace.config(), &json!({"agent":"new-original"})).unwrap();
    workspace.package("second", json!({"agent":"managed"}));
    let second = workspace.install("second");
    install::enable(&workspace.0, first["id"].as_str().unwrap()).unwrap();
    install::uninstall(&workspace.0, second["id"].as_str().unwrap()).unwrap();
    install::uninstall(&workspace.0, first["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        storage::read::<Value>(&workspace.config()).unwrap(),
        json!({"agent":"new-original"})
    );
}

#[test]
fn configuration_case_collision_refuses_before_installation_mutation() {
    let workspace = Workspace::new();
    workspace.package("settings", json!({"agent":"managed"}));
    let report = workspace
        .0
        .join("packages/claude-code/settings/compatibility.json");
    let mut value: Value = storage::read(&report).unwrap();
    let mut other = value["components"][0].clone();
    other["metadata"]["install_contribution"]["project_path"] = json!(".claude/Settings.json");
    value["components"].as_array_mut().unwrap().push(other);
    storage::write(&report, &value).unwrap();
    assert_eq!(
        install::install(
            &workspace.0,
            "settings",
            "claude-code",
            &workspace.0.join("project"),
            false
        )
        .unwrap_err()
        .code,
        "config-collision"
    );
    assert!(!workspace.config().exists());
    assert!(install::receipts(&workspace.0).unwrap().is_empty());
}

#[test]
fn raw_payload_to_managed_configuration_refuses_without_partial_journal() {
    let workspace = Workspace::new();
    let package = workspace.0.join("packages/amp-native/native");
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("index.ts"), b"export default () => {};\n").unwrap();
    fs::create_dir_all(package.join("skills/shared")).unwrap();
    fs::write(
        package.join("skills/shared/SKILL.md"),
        b"---\nname: shared\ndescription: test\n---\n",
    )
    .unwrap();
    fs::write(package.join("settings.json"), b"{\"agent\":\"original\"}\n").unwrap();
    storage::write(
        &package.join("compatibility.json"),
        &json!({"supported":true,"components":[{"kind":"plugins"}]}),
    )
    .unwrap();
    let receipt = install::install(
        &workspace.0,
        "native",
        "amp-native",
        &workspace.0.join("project"),
        false,
    )
    .unwrap();
    let physical = workspace
        .0
        .join("project/.amp/plugins/native/settings.json");
    let before = fs::read(&physical).unwrap();
    storage::write(&package.join("compatibility.json"),&json!({"supported":true,"components":[{"kind":"settings","metadata":{"native_target":"amp-native","install_contribution":{"project_path":".amp/plugins/native/settings.json","payload_path":"settings.json"}}}]})).unwrap();
    assert_eq!(
        install::install(
            &workspace.0,
            "native",
            "amp-native",
            &workspace.0.join("project"),
            false
        )
        .unwrap_err()
        .code,
        "ownership-conflict"
    );
    assert_eq!(fs::read(&physical).unwrap(), before);
    assert_eq!(
        install::receipts(&workspace.0).unwrap()[0].operation,
        receipt["operation"].as_str().unwrap()
    );
}

#[test]
fn update_and_uninstall_restore_managed_baseline_without_erasing_user_changes() {
    let workspace = Workspace::new();
    storage::write(
        &workspace.config(),
        &json!({"theme":"light","permissions":{"allow":["original"]},"empty":{}}),
    )
    .unwrap();
    workspace.package("settings",json!({"permissions":{"allow":["managed"]},"mcpServers":{"test":{"command":"first-party"}}}));
    let first = workspace.install("settings");
    let mut current: Value = storage::read(&workspace.config()).unwrap();
    current["theme"] = json!("dark");
    current["permissions"]["deny"] = json!(["user-rule"]);
    storage::write(&workspace.config(), &current).unwrap();
    workspace.package("settings", json!({"permissions":{"allow":["updated"]}}));
    let updated = workspace.install("settings");
    let current: Value = storage::read(&workspace.config()).unwrap();
    assert_eq!(
        current,
        json!({"theme":"dark","permissions":{"allow":["updated"],"deny":["user-rule"]},"empty":{}})
    );
    install::uninstall(&workspace.0, updated["id"].as_str().unwrap()).unwrap();
    let current: Value = storage::read(&workspace.config()).unwrap();
    assert_eq!(
        current,
        json!({"theme":"dark","permissions":{"allow":["original"],"deny":["user-rule"]},"empty":{}})
    );
    assert_eq!(first["id"], updated["id"]);
}

#[test]
fn managed_user_edit_conflicts_without_mutating_payload_or_receipt() {
    let workspace = Workspace::new();
    workspace.package("settings", json!({"agent":"managed"}));
    let receipt = workspace.install("settings");
    let id = receipt["id"].as_str().unwrap();
    let receipt_before = install::receipts(&workspace.0).unwrap()[0]
        .operation
        .clone();
    storage::write(
        &workspace.config(),
        &json!({"agent":"user","unrelated":true}),
    )
    .unwrap();
    let before = fs::read(workspace.config()).unwrap();
    for action in ["update", "disable", "uninstall"] {
        let error = match action {
            "update" => install::install(
                &workspace.0,
                "settings",
                "claude-code",
                &workspace.0.join("project"),
                false,
            )
            .unwrap_err(),
            "disable" => install::disable(&workspace.0, id).unwrap_err(),
            _ => install::uninstall(&workspace.0, id).unwrap_err(),
        };
        assert_eq!(error.code, "config-conflict");
        assert_eq!(fs::read(workspace.config()).unwrap(), before);
        assert_eq!(
            install::receipts(&workspace.0).unwrap()[0].operation,
            receipt_before
        );
    }
}

#[test]
fn shared_config_owners_disable_enable_and_rollback_keep_original_empty_parent() {
    let workspace = Workspace::new();
    storage::write(
        &workspace.config(),
        &json!({"mcpServers":{},"unrelated":"original"}),
    )
    .unwrap();
    for id in ["first", "second"] {
        workspace.package(id, json!({"mcpServers":{"test":{"command":"first-party"}}}));
    }
    let first = workspace.install("first");
    let second = workspace.install("second");
    install::disable(&workspace.0, first["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        storage::read::<Value>(&workspace.config()).unwrap()["mcpServers"]["test"]["command"],
        "first-party"
    );
    let disabled = install::disable(&workspace.0, second["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        storage::read::<Value>(&workspace.config()).unwrap(),
        json!({"mcpServers":{},"unrelated":"original"})
    );
    transaction::rollback(&workspace.0, disabled["operation"].as_str().unwrap()).unwrap();
    assert_eq!(
        storage::read::<Value>(&workspace.config()).unwrap()["mcpServers"]["test"]["command"],
        "first-party"
    );
    install::enable(&workspace.0, first["id"].as_str().unwrap()).unwrap();
    install::uninstall(&workspace.0, first["id"].as_str().unwrap()).unwrap();
    install::uninstall(&workspace.0, second["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        storage::read::<Value>(&workspace.config()).unwrap(),
        json!({"mcpServers":{},"unrelated":"original"})
    );
}

#[test]
fn explicit_native_payload_contribution_preserves_keys_without_conversion() {
    let workspace = Workspace::new();
    let package = workspace.0.join("packages/cline/native-mcp");
    fs::create_dir_all(&package).unwrap();
    let payload = b"{\"mcpServers\":{\"test\":{\"type\":\"streamableHttp\",\"url\":\"https://example.com/mcp\",\"disabled\":true}}}\n";
    fs::write(package.join("mcp-config-contribution.json"), payload).unwrap();
    storage::write(&package.join("compatibility.json"),&json!({"supported":true,"components":[{"kind":"mcps","metadata":{"native_target":"cline","install_contribution":{"project_path":".cline/mcp.json","payload_path":"mcp-config-contribution.json"}}}]})).unwrap();
    let receipt = install::install(
        &workspace.0,
        "native-mcp",
        "cline",
        &workspace.0.join("project"),
        false,
    )
    .unwrap();
    assert!(receipt["files"].as_object().unwrap().is_empty());
    assert_eq!(
        storage::read::<Value>(&workspace.0.join("project/.cline/mcp.json")).unwrap(),
        serde_json::from_slice::<Value>(payload).unwrap()
    );
    install::uninstall(&workspace.0, receipt["id"].as_str().unwrap()).unwrap();
    assert!(!workspace.0.join("project/.cline/mcp.json").exists());
}
