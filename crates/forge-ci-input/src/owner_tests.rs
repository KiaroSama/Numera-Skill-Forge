#[cfg(target_os = "linux")]
#[test]
fn owned_descendant_is_terminated_at_idle_deadline() {
    use crate::owner;
    use std::{fs, path::PathBuf};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.ci-work/input-owner")
        .join(forge_core::transaction::operation_id());
    fs::create_dir_all(&root).unwrap();
    let mut command = owner::command("/bin/sh");
    command
        .args(["-c", "sleep 30 & echo $! > child.pid; wait"])
        .current_dir(&root);
    let result = owner::run(command, 3, 1);
    assert!(result.is_err());
    let pid: i32 = fs::read_to_string(root.join("child.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // A dead zombie is not executing; Linux init may not reap immediately.
    #[cfg(target_os = "linux")]
    if let Ok(status) = fs::read_to_string(format!("/proc/{pid}/status")) {
        assert!(status.lines().any(|l| l.starts_with("State:\tZ")));
    }
    #[cfg(not(target_os = "linux"))]
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    fs::remove_dir_all(root).unwrap();
}
