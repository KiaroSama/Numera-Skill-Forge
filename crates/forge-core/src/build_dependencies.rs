use crate::{
    Error, Result,
    model::{Accepted, Lock, Package, Registry},
};
use std::collections::BTreeSet;

pub fn resolve<'a>(
    registry: &'a Registry,
    lock: &'a Lock,
    id: &str,
    target: &str,
) -> Result<Vec<(&'a Package, &'a Accepted)>> {
    let mut visiting = BTreeSet::new();
    let mut completed = BTreeSet::new();
    let mut result = Vec::new();
    visit(
        registry,
        lock,
        id,
        target,
        &mut visiting,
        &mut completed,
        &mut result,
    )?;
    Ok(result)
}
fn visit<'a>(
    registry: &'a Registry,
    lock: &'a Lock,
    id: &str,
    target: &str,
    visiting: &mut BTreeSet<String>,
    completed: &mut BTreeSet<String>,
    result: &mut Vec<(&'a Package, &'a Accepted)>,
) -> Result<()> {
    if completed.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id.to_owned()) {
        return Err(Error::new("dependency-cycle", "Package dependency cycle"));
    }
    let package = registry
        .packages
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| Error::new("missing-dependency", format!("Unregistered package {id}")))?;
    if !package.targets.is_empty() && !package.targets.iter().any(|value| value == target) {
        return Err(Error::new(
            "dependency-target",
            format!("Package {id} does not permit target {target}"),
        ));
    }
    let accepted = lock.sources.get(&package.source).ok_or_else(|| {
        Error::new(
            "approval-required",
            format!(
                "Dependency source {} has no accepted snapshot",
                package.source
            ),
        )
    })?;
    if accepted.approved_digest != accepted.snapshot.content_digest {
        return Err(Error::new(
            "integrity",
            "Accepted source digest differs from approval",
        ));
    }
    for dependency in &package.dependencies {
        visit(
            registry, lock, dependency, target, visiting, completed, result,
        )?;
    }
    visiting.remove(id);
    completed.insert(id.to_owned());
    result.push((package, accepted));
    Ok(())
}
