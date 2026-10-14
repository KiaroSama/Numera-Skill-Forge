use std::process::Command;

#[test]
fn ambiguous_or_unsupported_requests_never_launch_a_job() {
    for args in [
        vec![
            "run", "--image", "latest", "--source", "main", "--mode", "core", "--output", "unused",
        ],
        vec![
            "project",
            "--repository",
            ".",
            "--revision",
            "main",
            "--output",
            "unused",
        ],
        vec![
            "project",
            "--repository",
            ".",
            "--revision",
            "main",
            "--revision",
            "main",
        ],
        vec!["worker", "arbitrary-shell"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_forge-ci-input"))
            .args(args)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(
            String::from_utf8(result.stderr)
                .unwrap()
                .contains("refused or failed")
        );
    }
}
