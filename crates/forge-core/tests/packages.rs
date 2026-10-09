use forge_core::{
    adapters,
    model::{Component, Kind},
    storage,
};
use std::collections::BTreeMap;
#[test]
fn required_hook_is_not_silently_downgraded_to_prompt() {
    let hook = Component {
        id: "demo:hooks/check.js".into(),
        name: "check".into(),
        kind: Kind::Hooks,
        root: "hooks/check.js".into(),
        required: true,
        metadata: BTreeMap::new(),
    };
    for target in adapters::capabilities() {
        let result = adapters::assess(std::slice::from_ref(&hook), &target.harness).unwrap();
        assert_eq!(result["supported"], false);
        assert_eq!(result["unsupported"][0]["required"], true);
    }
}
#[test]
fn digest_matches_standard_sha256_vector() {
    assert_eq!(
        storage::digest(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
