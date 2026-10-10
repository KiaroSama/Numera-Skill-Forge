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
    credential.credential_reference = Some("FORGE_READ_TOKEN".into());
    registry::validate(&Registry {
        sources: vec![credential.clone()],
        ..Registry::default()
    })
    .unwrap();
    assert_eq!(
        source::candidate(Path::new("unused-provider-state"), &credential, false)
            .unwrap_err()
            .code,
        "credential-provider-unavailable"
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
