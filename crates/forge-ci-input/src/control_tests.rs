use crate::{contract, sandbox};
use serde_json::{Value, json};

fn configured() -> Value {
    json!({"Image":"sha256:fixture","Config":{"User":"1000:1000","OpenStdin":false,"Tty":false,"Labels":{"numera.forge.input.owner":"fixture"}},
        "HostConfig":{"NetworkMode":"none","ReadonlyRootfs":true,"Privileged":false,
        "Memory":4294967296u64,"MemorySwap":4294967296u64,"NanoCpus":2000000000u64,"PidsLimit":256,
        "PidMode":"","IpcMode":"private","UTSMode":"","CapDrop":["ALL"],"SecurityOpt":["no-new-privileges"],
        "Tmpfs":sandbox::WRITES.iter().map(|p| ((*p).to_owned(),json!("rw,exec,nosuid,nodev,size=2147483648,mode=1777"))).collect::<serde_json::Map<_,_>>()},"Mounts":[]})
}

#[test]
fn missing_or_relaxed_controls_are_refused_before_product_startup() {
    let valid = configured();
    sandbox::controls(&valid, "sha256:fixture", "fixture").unwrap();
    for (pointer, replacement) in [
        ("/Image", json!("sha256:other")),
        ("/Config/User", json!("0")),
        ("/Config/OpenStdin", json!(true)),
        ("/Config/Tty", json!(true)),
        ("/HostConfig/ReadonlyRootfs", json!(false)),
        ("/HostConfig/NetworkMode", json!("host")),
        ("/HostConfig/Privileged", json!(true)),
        ("/HostConfig/Memory", json!(0)),
        ("/HostConfig/MemorySwap", json!(-1)),
        ("/HostConfig/NanoCpus", json!(0)),
        ("/HostConfig/PidsLimit", json!(0)),
        ("/HostConfig/PidMode", json!("host")),
        ("/HostConfig/IpcMode", json!("host")),
        ("/HostConfig/CapDrop", json!([])),
        ("/HostConfig/SecurityOpt", json!(["seccomp=unconfined"])),
        ("/HostConfig/Tmpfs", json!({})),
        ("/Mounts", json!([{"Type":"bind","Destination":"/input"}])),
    ] {
        let mut tampered = valid.clone();
        *tampered.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            sandbox::controls(&tampered, "sha256:fixture", "fixture").is_err(),
            "{pointer}"
        );
    }
    let mut unbounded = valid;
    unbounded["HostConfig"]["Tmpfs"]["/scratch"] =
        json!("rw,nosuid,nodev,size=999999999999,mode=1777");
    assert!(sandbox::controls(&unbounded, "sha256:fixture", "fixture").is_err());
}

#[test]
fn manifest_rejects_unsafe_modes_paths_order_and_overflow() {
    let file = json!({"path":"Cargo.toml","mode":"100644","bytes":1,"sha256":"a".repeat(64)});
    let valid = json!({"schema":1,"contract":contract::CONTRACT,"source_revision":"a".repeat(40),"files":[file]});
    contract::validate(&serde_json::from_value(valid.clone()).unwrap()).unwrap();
    for (key, value) in [
        ("path", json!("crates/../secrets.md")),
        ("mode", json!("120000")),
        ("bytes", json!(contract::MAX_SOURCE + 1)),
        ("sha256", json!("Z".repeat(64))),
    ] {
        let mut altered = valid.clone();
        altered["files"][0][key] = value;
        assert!(contract::validate(&serde_json::from_value(altered).unwrap()).is_err());
    }
    let mut duplicate = valid;
    duplicate["files"].as_array_mut().unwrap().push(file);
    assert!(contract::validate(&serde_json::from_value(duplicate).unwrap()).is_err());
}

#[test]
fn cancellation_during_cleanup_cannot_publish_success() {
    assert_eq!(
        sandbox::final_outcome("succeeded", true, false),
        "succeeded"
    );
    assert_eq!(
        sandbox::final_outcome("succeeded", true, true),
        "terminated"
    );
    assert_eq!(sandbox::final_outcome("succeeded", false, true), "failed");
    assert_eq!(sandbox::final_outcome("refused", true, false), "refused");
}
