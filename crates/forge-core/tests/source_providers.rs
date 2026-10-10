use forge_core::{
    model::{Registry, Source},
    registry, source,
};
use serde_json::json;
use std::path::Path;

fn registered(repository: &str, policy: serde_json::Value) -> Source {
    serde_json::from_value(json!({
        "id": "provider-demo", "repository": repository,
        "metadata": {"provider_policy": policy}
    }))
    .unwrap()
}

#[test]
fn credential_and_ssh_registration_never_pretend_authenticated_acquisition() {
    let policy = json!({"transport":"https", "host":"forge.example.invalid", "reviewed":true});
    let mut credential = registered("https://forge.example.invalid/team/skills.git", policy);
    credential.credential_reference = Some(format!("FORGE_TEST_MISSING_{}", std::process::id()));
    registry::validate(&Registry {
        sources: vec![credential.clone()],
        ..Registry::default()
    })
    .unwrap();
    assert_eq!(
        source::candidate(Path::new("unused-provider-state"), &credential, false)
            .unwrap_err()
            .code,
        "credential-missing"
    );
    for key in [
        "",
        "token:value",
        "env:FORGE_TOKEN",
        "../secret",
        "1TOKEN",
        "TOKEN\n",
    ] {
        credential.credential_reference = Some(key.into());
        assert_eq!(
            registry::validate(&Registry {
                sources: vec![credential.clone()],
                ..Registry::default()
            })
            .unwrap_err()
            .code,
            "credential-reference"
        );
    }
    let ssh = registered(
        "ssh://git@forge.example.invalid/team/skills.git",
        json!({"transport":"ssh", "host":"forge.example.invalid", "reviewed":true}),
    );
    registry::validate(&Registry {
        sources: vec![ssh.clone()],
        ..Registry::default()
    })
    .unwrap();
    assert_eq!(
        source::candidate(Path::new("unused-provider-state"), &ssh, false)
            .unwrap_err()
            .code,
        "ssh-provider-unavailable"
    );
    assert!(registry::validate_repository(&ssh.repository).is_err());
}

#[test]
fn isolated_git_accepts_child_only_header_and_does_not_persist_it() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.ci-work/tests")
        .join(forge_core::transaction::operation_id());
    std::fs::create_dir_all(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    let _cleanup = Cleanup(root.clone());
    let home = root.to_str().unwrap();
    let result = forge_core::process::git_isolated(&["-C".into(),home.into(),"--config-env=http.https://example.invalid/team/repository.extraHeader=FORGE_TEST_HEADER".into(),"config".into(),"--get-urlmatch".into(),"http.extraHeader".into(),"https://example.invalid/team/repository".into()], 15, &[("HOME",home),("FORGE_TEST_HEADER","Authorization: Bearer first-party-fixture-not-a-secret")]).unwrap();
    assert_eq!(
        result,
        b"Authorization: Bearer first-party-fixture-not-a-secret\n"
    );
    assert!(std::fs::read_dir(&root).unwrap().next().is_none());
    let missing = forge_core::process::git_isolated(
        &[
            "--config-env=http.extraHeader=FORGE_TEST_HEADER".into(),
            "config".into(),
            "--get".into(),
            "http.extraHeader".into(),
        ],
        15,
        &[("HOME", home)],
    )
    .unwrap_err();
    assert_eq!(missing.code, "git-failed");
    assert!(!missing.message.contains("first-party-fixture"));
}

#[test]
fn ssh_explicit_references_are_typed_and_cannot_be_http_credentials() {
    let mut ssh = registered(
        "ssh://git@forge.example.invalid/team/skills.git",
        json!({"transport":"ssh","host":"forge.example.invalid","reviewed":true}),
    );
    ssh.metadata.insert("ssh_authentication".into(), json!({"public_key_reference":"FORGE_PUBLIC_KEY","known_hosts_reference":"FORGE_KNOWN_HOSTS","agent_reference":"FORGE_AGENT"}));
    registry::validate(&Registry {
        sources: vec![ssh.clone()],
        ..Registry::default()
    })
    .unwrap();
    ssh.metadata.get_mut("ssh_authentication").unwrap()["agent_reference"] = json!("$(execute)");
    assert_eq!(
        registry::validate(&Registry {
            sources: vec![ssh.clone()],
            ..Registry::default()
        })
        .unwrap_err()
        .code,
        "ssh-policy"
    );
    ssh.metadata.get_mut("ssh_authentication").unwrap()["agent_reference"] = json!("FORGE_AGENT");
    ssh.credential_reference = Some("FORGE_HTTP_HEADER".into());
    assert_eq!(
        registry::validate(&Registry {
            sources: vec![ssh],
            ..Registry::default()
        })
        .unwrap_err()
        .code,
        "ssh-policy"
    );
}

#[test]
fn custom_provider_requires_exact_reviewed_host_and_safe_repository() {
    let policy = json!({"transport":"https", "host":"forge.example.invalid", "reviewed":true});
    let source = registered(
        "https://forge.example.invalid/team/skills.git",
        policy.clone(),
    );
    registry::validate(&Registry {
        sources: vec![source],
        ..Registry::default()
    })
    .unwrap();
    for (repository, policy) in [
        ("https://forge.example.invalid/team/skills.git", json!(null)),
        (
            "https://other.example.invalid/team/skills.git",
            policy.clone(),
        ),
        (
            "https://forge.example.invalid/team/skills.git",
            json!({"transport":"https","host":"forge.example.invalid","reviewed":false}),
        ),
        (
            "https://forge.example.invalid:443/team/skills.git",
            policy.clone(),
        ),
        (
            "https://token@forge.example.invalid/team/skills.git",
            policy.clone(),
        ),
        (
            "https://forge.example.invalid/team/%2e%2e/skills.git",
            policy.clone(),
        ),
        (
            "https://forge.example.invalid/team/../skills.git",
            policy.clone(),
        ),
        (
            "https://forge.example.invalid/team/skills.git?token=example",
            policy.clone(),
        ),
        ("https://forge.example.invalid/team/skills.git\0", policy),
    ] {
        assert!(
            registry::validate(&Registry {
                sources: vec![registered(repository, policy)],
                ..Registry::default()
            })
            .is_err()
        );
    }
}
