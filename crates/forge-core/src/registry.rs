use crate::{
    Error, Result,
    model::{Lock, Package, Registry, Source},
    storage, validation,
};
use std::{collections::BTreeSet, path::Path};

pub fn load(root: &Path) -> Result<Registry> {
    let path = root.join("registry/sources.json");
    let registry = if path.exists() {
        storage::read(&path)?
    } else {
        Registry::default()
    };
    validate(&registry)?;
    Ok(registry)
}
pub fn save(root: &Path, registry: &Registry) -> Result<()> {
    validate(registry)?;
    storage::write(&root.join("registry/sources.json"), registry)
}
pub fn lock(root: &Path) -> Result<Lock> {
    let p = root.join("forge.lock.json");
    if p.exists() {
        let lock: Lock = storage::read(&p)?;
        if lock.schema_version != 1 {
            return Err(Error::new(
                "schema-version",
                "Unsupported lock version; preserve state and migrate explicitly",
            ));
        }
        for (id, accepted) in &lock.sources {
            validation::identifier(id)?;
            if accepted.snapshot.source != *id
                || accepted.approved_digest != accepted.snapshot.content_digest
            {
                return Err(Error::new(
                    "integrity",
                    "Accepted snapshot identity differs from approval",
                ));
            }
        }
        Ok(lock)
    } else {
        Ok(Lock {
            schema_version: 1,
            ..Lock::default()
        })
    }
}
pub fn save_lock(root: &Path, lock: &Lock) -> Result<()> {
    storage::write(&root.join("forge.lock.json"), lock)
}
pub fn validate(registry: &Registry) -> Result<()> {
    if registry.schema_version != 1 {
        return Err(Error::new(
            "schema-version",
            "Unsupported registry version; migration required",
        ));
    }
    let mut ids = BTreeSet::new();
    for source in &registry.sources {
        validation::identifier(&source.id)?;
        if !ids.insert(&source.id) {
            return Err(Error::new("duplicate-id", "Source IDs must be unique"));
        }
        for p in source.roots.iter().chain(&source.excludes) {
            validation::relative_path(p)?;
        }
        crate::source::providers::validate(source)?;
    }
    let mut packages = BTreeSet::new();
    for p in &registry.packages {
        validation::identifier(&p.id)?;
        if !packages.insert(&p.id) {
            return Err(Error::new("duplicate-id", "Package IDs must be unique"));
        }
        if !ids.contains(&p.source) {
            return Err(Error::new(
                "missing-source",
                "Package references an unregistered source",
            ));
        }
    }
    for p in &registry.packages {
        closure(registry, &p.id, &mut BTreeSet::new(), &mut BTreeSet::new())?;
    }
    Ok(())
}
fn closure(
    r: &Registry,
    id: &str,
    visiting: &mut BTreeSet<String>,
    done: &mut BTreeSet<String>,
) -> Result<()> {
    if done.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id.to_owned()) {
        return Err(Error::new("dependency-cycle", "Package dependency cycle"));
    }
    let p =
        r.packages.iter().find(|p| p.id == id).ok_or_else(|| {
            Error::new("missing-dependency", format!("Unregistered package {id}"))
        })?;
    for d in &p.dependencies {
        closure(r, d, visiting, done)?;
    }
    visiting.remove(id);
    done.insert(id.to_owned());
    Ok(())
}
pub fn validate_repository(repository: &str) -> Result<()> {
    crate::source::providers::repository(repository, None)
}
pub fn add_source(root: &Path, source: Source) -> Result<()> {
    let mut r = load(root)?;
    r.sources.push(source);
    save(root, &r)
}
pub fn add_package(root: &Path, p: Package) -> Result<()> {
    let mut r = load(root)?;
    r.packages.push(p);
    save(root, &r)
}
