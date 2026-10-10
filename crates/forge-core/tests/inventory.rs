use forge_core::{discovery, model::Kind, storage, transaction};
use std::{fs, path::PathBuf};
#[test]
fn complete_native_roots_own_their_companions_without_duplicate_component_conversion() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.ci-work/tests")
        .join(transaction::operation_id());
    fs::create_dir_all(&root).unwrap();
    let inputs = [
        ("LICENSE", b"First-party GPL fixture".as_slice()),
        (
            "plugin/.claude-plugin/plugin.json",
            b"{\"name\":\"fixture\"}".as_slice(),
        ),
        (
            "plugin/skills/nested/SKILL.md",
            b"---\nname: nested\ndescription: original\n---\nPrompt\n".as_slice(),
        ),
        (
            "plugin/skills/nested/scripts/tool.py",
            b"raise RuntimeError('must never run')".as_slice(),
        ),
        (
            "skills/standalone/SKILL.md",
            b"---\nname: standalone\ndescription: fixture\n---\nPrompt\n".as_slice(),
        ),
        (
            "skills/standalone/.codex/public.json",
            b"{\"original\":true}".as_slice(),
        ),
    ];
    for (path, bytes) in inputs {
        storage::atomic(&root.join(path), bytes).unwrap();
    }
    let snapshot = discovery::inspect(&root, "fixture", "a").unwrap();
    assert_eq!(snapshot.components.len(), 2);
    assert!(
        snapshot
            .components
            .iter()
            .any(|c| c.kind == Kind::Plugins && c.root == "plugin")
    );
    assert!(
        snapshot
            .components
            .iter()
            .any(|c| c.kind == Kind::Skills && c.root == "skills/standalone")
    );
    assert!(
        snapshot
            .files
            .iter()
            .any(|f| f.path == "skills/standalone/.codex/public.json")
    );
    assert!(snapshot.files.iter().any(|f| f.path.ends_with("tool.py")));
    fs::remove_dir_all(root).unwrap();
}
