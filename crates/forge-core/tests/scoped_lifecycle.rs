use forge_core::{install, storage};
use serde_json::json;
use std::{fs, path::PathBuf};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(format!(
                "scoped-{}-{}",
                std::process::id(),
                forge_core::transaction::operation_id()
            ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn package(&self, id: &str) {
        let root = self.0.join(format!("packages/claude-code/{id}"));
        fs::create_dir_all(root.join("skills/shared/scripts")).unwrap();
        fs::write(
            root.join("skills/shared/SKILL.md"),
            b"---\nname: shared\ndescription: Shared test\n---\nHello\n",
        )
        .unwrap();
        fs::write(
            root.join("skills/shared/scripts/run.sh"),
            b"#!/bin/sh\nexit 0\n",
        )
        .unwrap();
        fs::write(root.join("LICENSE"), b"First-party test notice\n").unwrap();
        storage::write(&root.join("provenance.json"), &json!({"file_modes":{"skills/shared/SKILL.md":420,"skills/shared/scripts/run.sh":493,"LICENSE":420}})).unwrap();
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn conflicting_shared_update_and_disabled_owner_do_not_lose_original_bytes() {
    let workspace = Workspace::new();
    workspace.package("first");
    workspace.package("second");
    let destination = workspace.0.join("project");
    fs::create_dir(&destination).unwrap();
    let first =
        install::install(&workspace.0, "first", "claude-code", &destination, false).unwrap();
    let second =
        install::install(&workspace.0, "second", "claude-code", &destination, false).unwrap();
    install::disable(&workspace.0, second["id"].as_str().unwrap()).unwrap();
    fs::write(
        workspace
            .0
            .join("packages/claude-code/first/skills/shared/SKILL.md"),
        b"Changed upstream",
    )
    .unwrap();
    assert_eq!(
        install::install(&workspace.0, "first", "claude-code", &destination, false)
            .unwrap_err()
            .code,
        "ownership-conflict"
    );
    let file = destination.join(".claude/skills/shared/SKILL.md");
    assert_eq!(
        fs::read(&file).unwrap(),
        b"---\nname: shared\ndescription: Shared test\n---\nHello\n"
    );
    install::disable(&workspace.0, first["id"].as_str().unwrap()).unwrap();
    assert!(!file.exists());
    install::enable(&workspace.0, first["id"].as_str().unwrap()).unwrap();
    install::enable(&workspace.0, second["id"].as_str().unwrap()).unwrap();
    assert!(file.exists());
}

#[test]
fn oversized_provenance_mode_is_rejected_before_copying() {
    let workspace = Workspace::new();
    workspace.package("mode");
    storage::write(
        &workspace
            .0
            .join("packages/claude-code/mode/provenance.json"),
        &json!({"file_modes":{"skills/shared/SKILL.md":4294967716u64}}),
    )
    .unwrap();
    let destination = workspace.0.join("project");
    fs::create_dir(&destination).unwrap();
    assert_eq!(
        install::install(&workspace.0, "mode", "claude-code", &destination, false)
            .unwrap_err()
            .code,
        "mode"
    );
    assert!(!destination.join(".claude").exists());
}
#[cfg(unix)]
#[test]
fn owned_special_permission_changes_refuse_lifecycle_removal() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = Workspace::new();
    workspace.package("mode");
    let destination = workspace.0.join("project");
    fs::create_dir(&destination).unwrap();
    let receipt =
        install::install(&workspace.0, "mode", "claude-code", &destination, false).unwrap();
    let physical = destination.join(".claude/skills/shared/scripts/run.sh");
    fs::set_permissions(&physical, fs::Permissions::from_mode(0o4755)).unwrap();
    assert_eq!(
        install::uninstall(&workspace.0, receipt["id"].as_str().unwrap())
            .unwrap_err()
            .code,
        "mode-conflict"
    );
    assert!(physical.exists());
}

#[test]
fn explicit_home_scope_preserves_assets_and_collection_modes_without_personal_install() {
    let workspace = Workspace::new();
    workspace.package("home");
    let package = workspace.0.join("packages/claude-code/home");
    fs::write(
        package.join("skills/shared/binary.dat"),
        [0u8, 255, 13, 10, 0],
    )
    .unwrap();
    fs::create_dir_all(package.join("skills/shared/.codex")).unwrap();
    fs::write(
        package.join("skills/shared/.codex/native.json"),
        b"{\"original\":true}\n",
    )
    .unwrap();
    storage::write(&package.join("provenance.json"), &json!({"files":[{"output_path":"packages/claude-code/home/skills/shared/scripts/run.sh","mode":"100755"},{"output_path":"packages/claude-code/home/skills/shared/.codex/native.json","mode":"100644"}]})).unwrap();
    let home = workspace.0.join("isolated home");
    fs::create_dir(&home).unwrap();
    let preview = install::install_scoped(
        &workspace.0,
        "home",
        "claude-code",
        &home,
        install::Scope::User,
        true,
    )
    .unwrap();
    assert_eq!(preview["scope"], "user");
    assert!(!home.join(".claude").exists());
    let receipt = install::install_scoped(
        &workspace.0,
        "home",
        "claude-code",
        &home,
        install::Scope::User,
        false,
    )
    .unwrap();
    assert_eq!(
        receipt["modes"][".claude/skills/shared/scripts/run.sh"],
        493
    );
    assert_eq!(
        fs::read(home.join(".claude/skills/shared/binary.dat")).unwrap(),
        [0u8, 255, 13, 10, 0]
    );
    assert_eq!(
        fs::read(home.join(".claude/skills/shared/.codex/native.json")).unwrap(),
        b"{\"original\":true}\n"
    );
    forge_core::transaction::rollback(&workspace.0, receipt["operation"].as_str().unwrap())
        .unwrap();
    assert!(!home.join(".claude/skills/shared/binary.dat").exists());
    assert!(install::receipts(&workspace.0).unwrap().is_empty());
}

#[test]
fn verified_native_directory_copies_complete_package_without_running_entry() {
    let workspace = Workspace::new();
    let package = workspace.0.join("packages/amp-native/native");
    fs::create_dir_all(package.join("assets")).unwrap();
    fs::write(
        package.join("index.ts"),
        b"throw new Error('must never execute');\n",
    )
    .unwrap();
    fs::write(package.join("assets/input.bin"), [1u8, 0, 255]).unwrap();
    storage::write(&package.join("compatibility.json"),&json!({"supported":true,"components":[{"kind":"plugins","metadata":{"native_target":"amp-native"}}]})).unwrap();
    let destination = workspace.0.join("project");
    fs::create_dir(&destination).unwrap();
    let receipt =
        install::install(&workspace.0, "native", "amp-native", &destination, false).unwrap();
    assert!(
        receipt["delivery"]
            .as_str()
            .unwrap()
            .starts_with("native-directory")
    );
    assert_eq!(
        fs::read(destination.join(".amp/plugins/native/index.ts")).unwrap(),
        b"throw new Error('must never execute');\n"
    );
    assert_eq!(
        fs::read(destination.join(".amp/plugins/native/assets/input.bin")).unwrap(),
        [1u8, 0, 255]
    );
    let package = workspace.0.join("packages/claude-code/native");
    fs::create_dir_all(&package).unwrap();
    storage::write(
        &package.join("compatibility.json"),
        &json!({"supported":true,"components":[{"kind":"hooks"}]}),
    )
    .unwrap();
    assert_eq!(
        install::install(&workspace.0, "native", "claude-code", &destination, false)
            .unwrap_err()
            .code,
        "native-install-required"
    );
    assert!(!destination.join(".claude").exists());
    let invalid = workspace.0.join("packages/antigravity/invalid");
    fs::create_dir_all(&invalid).unwrap();
    fs::write(invalid.join("plugin.json"), b"not JSON").unwrap();
    storage::write(
        &invalid.join("compatibility.json"),
        &json!({"supported":true,"components":[{"kind":"plugins"}]}),
    )
    .unwrap();
    assert_eq!(
        install::install(&workspace.0, "invalid", "antigravity", &destination, false)
            .unwrap_err()
            .code,
        "json"
    );
    assert!(!destination.join(".agents").exists());
}

#[test]
fn identical_packages_share_files_and_uninstall_retains_enabled_owner() {
    let workspace = Workspace::new();
    workspace.package("first");
    workspace.package("second");
    let destination = workspace.0.join("project سلام");
    fs::create_dir(&destination).unwrap();
    let first =
        install::install(&workspace.0, "first", "claude-code", &destination, false).unwrap();
    let second =
        install::install(&workspace.0, "second", "claude-code", &destination, false).unwrap();
    let path = destination.join(".claude/skills/shared/scripts/run.sh");
    assert_eq!(fs::read(&path).unwrap(), b"#!/bin/sh\nexit 0\n");
    install::uninstall(&workspace.0, first["id"].as_str().unwrap()).unwrap();
    assert!(path.exists());
    install::disable(&workspace.0, second["id"].as_str().unwrap()).unwrap();
    assert!(!path.exists());
    install::enable(&workspace.0, second["id"].as_str().unwrap()).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"#!/bin/sh\nexit 0\n");
    assert_eq!(
        fs::read(destination.join(".claude/skills/.forge-packages/second/LICENSE")).unwrap(),
        b"First-party test notice\n"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
}
