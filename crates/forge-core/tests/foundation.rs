use forge_core::validation::{identifier, relative_path};

#[test]
fn registration_refuses_unsafe_portable_paths() {
    for path in [
        "../escape",
        "/root",
        "C:/outside",
        "file:stream",
        "CON",
        "a/NUL.txt",
        "a\\b",
        "x.",
    ] {
        assert!(relative_path(path).is_err(), "accepted {path}");
    }
    for path in [
        "skills/hello/SKILL.md",
        "assets/سلام.png",
        "folder with spaces/file.txt",
    ] {
        assert!(relative_path(path).is_ok(), "rejected {path}");
    }
    assert!(identifier("hello-skill").is_ok());
    assert!(identifier("../escape").is_err());
}
