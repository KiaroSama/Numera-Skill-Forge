use forge_core::{
    adapters,
    model::{Component, Kind},
    storage,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn component(kind: Kind, root: &str, native: Option<&str>) -> Component {
    let mut metadata = BTreeMap::new();
    if let Some(target) = native {
        metadata.insert("native_target".into(), json!(target));
    }
    Component {
        id: "source:unit".into(),
        name: "unit".into(),
        kind,
        root: root.into(),
        required: true,
        metadata,
    }
}

#[test]
fn refuses_cross_host_hooks_and_allows_only_explicit_native_identity() {
    let generic = component(Kind::Hooks, "hooks/hooks.json", None);
    assert_eq!(
        adapters::assess(std::slice::from_ref(&generic), "claude-code").unwrap()["supported"],
        false
    );
    let native = component(Kind::Hooks, "hooks/hooks.json", Some("claude-code"));
    assert_eq!(
        adapters::assess(std::slice::from_ref(&native), "claude-code").unwrap()["supported"],
        true
    );
    assert_eq!(
        adapters::assess(&[native], "cursor").unwrap()["supported"],
        false
    );
    let mut optional = generic;
    optional.required = false;
    let report = adapters::assess(&[optional], "claude-code").unwrap();
    assert_eq!(report["supported"], true);
    assert_eq!(report["unsupported"][0]["required"], false);
}

#[test]
fn maps_native_components_without_inventing_shared_runtime_formats() {
    assert_eq!(
        adapters::component_path(
            "claude-code",
            &component(Kind::Mcps, ".mcp.json", Some("claude-code"))
        )
        .unwrap(),
        ".mcp.json"
    );
    assert_eq!(
        adapters::component_path(
            "antigravity",
            &component(Kind::Hooks, "hooks.json", Some("antigravity"))
        )
        .unwrap(),
        "hooks.json"
    );
    assert_eq!(
        adapters::component_path(
            "gemini",
            &component(Kind::Commands, "commands/review.toml", Some("gemini"))
        )
        .unwrap(),
        "commands/review.toml"
    );
    assert!(
        adapters::component_path(
            "gemini",
            &component(Kind::Commands, "commands/review.md", Some("gemini"))
        )
        .is_err()
    );
    assert!(
        adapters::component_path(
            "continue",
            &component(Kind::Plugins, "bundle", Some("continue"))
        )
        .is_err()
    );
    assert!(adapters::assess(&[], "not-a-host").is_err());
}

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.forge-state/adapter-tests")
            .join(format!("{label}-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn preserves_native_manifest_and_refuses_missing_runtime_entrypoints() {
    let scratch = Scratch::new("preserve");
    let bytes = b"{\"name\":\"source-plugin\",\"version\":\"4.0.0\",\"userConfig\":{}}\n";
    storage::atomic(&scratch.0.join(".claude-plugin/plugin.json"), bytes).unwrap();
    let plugin = component(Kind::Plugins, "plugin-root", Some("claude-code"));
    adapters::render(
        &scratch.0,
        "generated",
        "Generated",
        "0.1.0",
        "claude-code",
        &[plugin],
    )
    .unwrap();
    assert_eq!(
        storage::bytes(&scratch.0.join(".claude-plugin/plugin.json")).unwrap(),
        bytes
    );

    let scratch = Scratch::new("hermes");
    storage::atomic(
        &scratch.0.join("plugin.yaml"),
        b"name: unit\nversion: 1.0.0\n",
    )
    .unwrap();
    let plugin = component(Kind::Plugins, "plugin-root", Some("hermes-native"));
    assert!(
        adapters::render(
            &scratch.0,
            "unit",
            "Unit",
            "1.0.0",
            "hermes-native",
            std::slice::from_ref(&plugin)
        )
        .is_err()
    );
    storage::atomic(
        &scratch.0.join("__init__.py"),
        b"def register(ctx):\n    pass\n",
    )
    .unwrap();
    adapters::render(
        &scratch.0,
        "unit",
        "Unit",
        "1.0.0",
        "hermes-native",
        &[plugin],
    )
    .unwrap();
}

#[test]
fn codex_native_hooks_never_emit_portable_shadow_manifest() {
    let scratch = Scratch::new("codex");
    storage::write(
        &scratch.0.join("hooks/hooks.json"),
        &json!({"hooks": {"Stop": []}}),
    )
    .unwrap();
    let hook = component(Kind::Hooks, "hooks/hooks.json", Some("codex"));
    adapters::render(&scratch.0, "unit", "Unit", "1.0.0", "codex", &[hook]).unwrap();
    assert!(scratch.0.join(".codex-plugin/plugin.json").is_file());
    assert!(!scratch.0.join("plugin.json").exists());
}

#[test]
fn capability_inventory_is_versioned_and_honest_about_host_testing() {
    let all = adapters::capabilities();
    for target in [
        "claude-code",
        "codex",
        "hermes-native",
        "cursor",
        "antigravity",
        "opencode-v1",
        "opencode-v2",
        "copilot",
        "vscode",
        "gemini",
        "amp-native",
        "pi",
        "kiro",
        "windsurf",
        "cline",
        "kilo",
        "continue",
        "openhands",
        "openclaw-native",
    ] {
        let row = all
            .iter()
            .find(|row| row.harness == target)
            .unwrap_or_else(|| panic!("missing {target}"));
        assert!(row.verification.contains("real-host-not-tested"));
        assert!(!row.restrictions.is_empty());
    }
    let config = all.iter().find(|row| row.harness == "continue").unwrap();
    assert_eq!(config.delivery, "native-config");
    let report: Value =
        adapters::assess(&[component(Kind::Plugins, "module", Some("kilo"))], "kilo").unwrap();
    assert_eq!(report["supported"], false);
}
