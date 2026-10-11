use forge_core::{discovery, registry, service, storage, transaction};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(transaction::operation_id());
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn register(&self, id: &str, native: Option<Value>) {
        service::execute(&self.0, json!({"action":"source-add","id":id,"repository":format!("local:{}", self.0.display())})).unwrap();
        let revision = "a".repeat(40);
        let snapshot = self
            .0
            .join(format!(".forge-state/snapshots/{id}/{revision}"));
        storage::atomic(
            &snapshot.join("LICENSE"),
            b"First-party fixture GPL-3.0-only\n",
        )
        .unwrap();
        storage::atomic(
            &snapshot.join("skills/hello/SKILL.md"),
            b"---\nname: hello\ndescription: Harmless catalog fixture\n---\nHello.\n",
        )
        .unwrap();
        if let Some(native) = native {
            storage::write(&snapshot.join(".claude-plugin/plugin.json"), &native).unwrap();
        }
        let snapshot = discovery::inspect(&snapshot, id, &revision).unwrap();
        storage::write(
            &self.0.join(format!(".forge-state/candidates/{id}.json")),
            &snapshot,
        )
        .unwrap();
        service::execute(
            &self.0,
            json!({"action":"approve","source":id,"digest":snapshot.content_digest}),
        )
        .unwrap();
        service::execute(&self.0, json!({"action":"package-add","package":{"id":id,"name":id,"source":id,"components":[],"targets":["claude-code","codex"]}})).unwrap();
    }
    fn build(&self, id: &str, target: &str) -> forge_core::Result<Value> {
        service::execute(
            &self.0,
            json!({"action":"build","package":id,"target":target}),
        )
    }
    fn read(&self, path: &str) -> Value {
        storage::read(&self.0.join(path)).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn generic_catalogs_are_deterministic_native_names_match_and_rollback_is_coherent() {
    let f = Fixture::new();
    f.register("zeta", None);
    f.register("alpha", Some(json!({"name":"original-native","version":"release-seven","description":"Original metadata"})));
    f.build("zeta", "claude-code").unwrap();
    let first_catalog = fs::read(f.0.join("catalogs/builds/claude-code.json")).unwrap();
    let first_market =
        fs::read(f.0.join("packages/claude-code/.claude-plugin/marketplace.json")).unwrap();
    let first_lock = fs::read(f.0.join("forge.lock.json")).unwrap();
    let first_owner = fs::read(f.0.join("catalogs/builds/claude-code.lock.json")).unwrap();
    let native_bytes = fs::read(
        f.0.join(".forge-state/snapshots/alpha")
            .join("a".repeat(40))
            .join(".claude-plugin/plugin.json"),
    )
    .unwrap();
    let second = f.build("alpha", "claude-code").unwrap();
    let catalog = f.read("catalogs/builds/claude-code.json");
    assert_eq!(catalog["target"], "claude-code");
    assert_eq!(catalog["packages"][0]["id"], "alpha");
    assert_eq!(catalog["packages"][1]["id"], "zeta");
    assert_eq!(catalog["packages"][0]["version"], "0.1.0");
    assert_eq!(catalog["packages"][0]["native_version"], "release-seven");
    let market = f.read("packages/claude-code/.claude-plugin/marketplace.json");
    assert_eq!(market["name"], "numera-forge-builds");
    assert_eq!(market["plugins"][0]["name"], "original-native");
    assert_eq!(market["plugins"][0]["source"], "./alpha");
    assert_eq!(market["plugins"][0]["version"], "release-seven");
    assert_eq!(
        fs::read(f.0.join("packages/claude-code/alpha/.claude-plugin/plugin.json")).unwrap(),
        native_bytes
    );
    let second_catalog = fs::read(f.0.join("catalogs/builds/claude-code.json")).unwrap();
    let second_market =
        fs::read(f.0.join("packages/claude-code/.claude-plugin/marketplace.json")).unwrap();
    let rebuilt = f.build("alpha", "claude-code").unwrap();
    assert_eq!(rebuilt["digest"], second["digest"]);
    assert_eq!(rebuilt["version"], second["version"]);
    assert_eq!(
        fs::read(f.0.join("catalogs/builds/claude-code.json")).unwrap(),
        second_catalog
    );
    assert_eq!(
        fs::read(f.0.join("packages/claude-code/.claude-plugin/marketplace.json")).unwrap(),
        second_market
    );
    service::execute(
        &f.0,
        json!({"action":"rollback","operation":rebuilt["operation"]}),
    )
    .unwrap();
    service::execute(
        &f.0,
        json!({"action":"rollback","operation":second["operation"]}),
    )
    .unwrap();
    assert_eq!(
        fs::read(f.0.join("catalogs/builds/claude-code.json")).unwrap(),
        first_catalog
    );
    assert_eq!(
        fs::read(f.0.join("packages/claude-code/.claude-plugin/marketplace.json")).unwrap(),
        first_market
    );
    assert_eq!(fs::read(f.0.join("forge.lock.json")).unwrap(), first_lock);
    assert_eq!(
        fs::read(f.0.join("catalogs/builds/claude-code.lock.json")).unwrap(),
        first_owner
    );
    assert!(
        !f.0.join("packages/claude-code/alpha/.claude-plugin/plugin.json")
            .exists()
    );
    assert_eq!(
        f.build("alpha", "claude-code").unwrap()["digest"],
        second["digest"]
    );
}

#[test]
fn catalog_edits_and_removals_refuse_without_replacing_owned_packages_or_lock() {
    for damaged in [
        "catalogs/builds/claude-code.json",
        "packages/claude-code/.claude-plugin/marketplace.json",
        "catalogs/builds/claude-code.lock.json",
    ] {
        for remove in [false, true] {
            let f = Fixture::new();
            f.register("first", None);
            f.register("second", None);
            f.build("first", "claude-code").unwrap();
            let lock = fs::read(f.0.join("forge.lock.json")).unwrap();
            let package =
                fs::read(f.0.join("packages/claude-code/first/.claude-plugin/plugin.json"))
                    .unwrap();
            let original = fs::read(f.0.join(damaged)).unwrap();
            if remove {
                fs::remove_file(f.0.join(damaged)).unwrap();
            } else {
                storage::atomic(&f.0.join(damaged), b"{\"user\":true}\n").unwrap();
            }
            assert!(f.build("second", "claude-code").is_err());
            assert_eq!(fs::read(f.0.join("forge.lock.json")).unwrap(), lock);
            assert_eq!(
                fs::read(f.0.join("packages/claude-code/first/.claude-plugin/plugin.json"))
                    .unwrap(),
                package
            );
            assert!(
                !f.0.join("packages/claude-code/second/.claude-plugin/plugin.json")
                    .exists()
            );
            assert!(!f.0.join(".forge-state/build/second-claude-code").exists());
            if remove {
                assert!(!f.0.join(damaged).exists());
            } else {
                assert_eq!(fs::read(f.0.join(damaged)).unwrap(), b"{\"user\":true}\n");
            }
            storage::atomic(&f.0.join(damaged), &original).unwrap();
            f.build("second", "claude-code").unwrap();
        }
    }
}

#[test]
fn native_catalog_names_collide_without_mutation_and_optional_versions_stay_absent() {
    let f = Fixture::new();
    f.register("first", Some(json!({"name":"shared-native"})));
    f.register(
        "second",
        Some(json!({"name":"shared-native","version":"2.0.0"})),
    );
    f.build("first", "claude-code").unwrap();
    let marketplace = f.read("packages/claude-code/.claude-plugin/marketplace.json");
    assert!(marketplace["plugins"][0].get("version").is_none());
    assert!(
        f.read("catalogs/builds/claude-code.json")["packages"][0]
            .get("native_version")
            .is_none()
    );
    let lock = fs::read(f.0.join("forge.lock.json")).unwrap();
    assert_eq!(
        f.build("second", "claude-code").unwrap_err().code,
        "catalog-collision"
    );
    assert_eq!(fs::read(f.0.join("forge.lock.json")).unwrap(), lock);
    assert_eq!(
        f.read("packages/claude-code/.claude-plugin/marketplace.json"),
        marketplace
    );
    assert!(
        !f.0.join("packages/claude-code/second/.claude-plugin/plugin.json")
            .exists()
    );
}

#[test]
fn public_catalog_ownership_survives_fresh_checkout_and_other_targets_stay_forge_only() {
    let f = Fixture::new();
    f.register("first", None);
    f.build("first", "claude-code").unwrap();
    let copy = Fixture::new();
    for relative in ["registry", "packages", "catalogs", "forge.lock.json"] {
        let source = f.0.join(relative);
        if source.is_file() {
            storage::atomic(&copy.0.join(relative), &fs::read(source).unwrap()).unwrap();
        } else {
            for (path, physical) in storage::payload_files(&source).unwrap() {
                storage::atomic(
                    &copy.0.join(relative).join(path),
                    &fs::read(physical).unwrap(),
                )
                .unwrap();
            }
        }
    }
    let original_catalog = fs::read(f.0.join("catalogs/builds/claude-code.json")).unwrap();
    let revision = "a".repeat(40);
    let snapshot = format!(".forge-state/snapshots/first/{revision}");
    for (path, physical) in storage::payload_files(&f.0.join(&snapshot)).unwrap() {
        storage::atomic(
            &copy.0.join(&snapshot).join(path),
            &fs::read(physical).unwrap(),
        )
        .unwrap();
    }
    copy.build("first", "claude-code").unwrap();
    assert_eq!(
        fs::read(copy.0.join("catalogs/builds/claude-code.json")).unwrap(),
        original_catalog
    );
    f.build("first", "codex").unwrap();
    assert_eq!(
        f.read("catalogs/builds/codex.json")["packages"][0]["path"],
        "packages/codex/first"
    );
    assert!(
        !f.0.join("packages/codex/.claude-plugin/marketplace.json")
            .exists()
    );
    assert_eq!(
        fs::read(f.0.join("catalogs/builds/claude-code.json")).unwrap(),
        original_catalog
    );
}

#[test]
fn generic_marketplace_coexists_with_collection_owned_catalog_and_refuses_unowned_takeover() {
    let f = Fixture::new();
    service::execute(&f.0, json!({"action":"collection-sync","apply":true})).unwrap();
    let root_market = fs::read(f.0.join(".claude-plugin/marketplace.json")).unwrap();
    let collection_catalog = fs::read(f.0.join("catalogs/collections.json")).unwrap();
    f.register("first", None);
    f.build("first", "claude-code").unwrap();
    assert_eq!(
        service::execute(&f.0, json!({"action":"collection-verify"})).unwrap()["verified"],
        true
    );
    assert_eq!(
        fs::read(f.0.join(".claude-plugin/marketplace.json")).unwrap(),
        root_market
    );
    assert_eq!(
        fs::read(f.0.join("catalogs/collections.json")).unwrap(),
        collection_catalog
    );
    let unowned = Fixture::new();
    unowned.register("first", None);
    storage::atomic(
        &unowned.0.join("catalogs/builds/claude-code.json"),
        b"{\"user\":true}\n",
    )
    .unwrap();
    let lock = registry::lock(&unowned.0).unwrap();
    assert_eq!(
        unowned.build("first", "claude-code").unwrap_err().code,
        "generated-conflict"
    );
    assert_eq!(
        storage::encoded(&registry::lock(&unowned.0).unwrap()).unwrap(),
        storage::encoded(&lock).unwrap()
    );
    assert_eq!(
        fs::read(unowned.0.join("catalogs/builds/claude-code.json")).unwrap(),
        b"{\"user\":true}\n"
    );
}
