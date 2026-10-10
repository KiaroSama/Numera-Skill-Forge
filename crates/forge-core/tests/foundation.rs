use forge_core::validation::{identifier, relative_path};

#[test]
fn operation_ids_remain_unique_across_parallel_same_clock_calls() {
    let threads = (0..2)
        .map(|_| {
            std::thread::spawn(|| {
                (0..128)
                    .map(|_| forge_core::transaction::operation_id())
                    .collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    let ids = threads
        .into_iter()
        .flat_map(|thread| thread.join().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), 256);
    assert!(ids.iter().all(|id| identifier(id).is_ok()));
}

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
