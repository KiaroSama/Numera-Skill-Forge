use crate::{
    Error, Result,
    config_merge::OwnedConfig,
    storage,
    transaction::{self, Change},
    validation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub use crate::lifecycle::{disable, enable, uninstall};
#[path = "installed_update.rs"]
mod installed_update;
mod planning;
pub use installed_update::update_installed;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    #[default]
    Project,
    User,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Receipt {
    pub id: String,
    pub package: String,
    pub target: String,
    pub destination: PathBuf,
    pub files: BTreeMap<String, String>,
    pub enabled: bool,
    pub operation: String,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub delivery: String,
    #[serde(default)]
    pub modes: BTreeMap<String, u32>,
    #[serde(default)]
    pub configs: BTreeMap<String, OwnedConfig>,
}

pub fn receipts(root: &Path) -> Result<Vec<Receipt>> {
    let directory = root.join(".forge-state/receipts");
    if !directory.exists() {
        return Ok(vec![]);
    }
    fs::read_dir(directory)?
        .filter_map(|entry| match entry {
            Ok(entry) if entry.path().extension().is_some_and(|ext| ext == "json") => {
                Some(storage::read(&entry.path()))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error.into())),
        })
        .collect()
}
pub(crate) fn receipt_path(root: &Path, id: &str) -> Result<PathBuf> {
    validation::identifier(id)?;
    let directory = root.join(".forge-state/receipts");
    if directory.exists() {
        validation::contained(&directory, &format!("{id}.json"))
    } else {
        Ok(directory.join(format!("{id}.json")))
    }
}
pub(crate) fn destination(path: &Path) -> Result<PathBuf> {
    if !path.is_dir() {
        return Err(Error::new(
            "destination",
            "Select an existing explicit project or home directory",
        ));
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    for ancestor in absolute.ancestors() {
        validation::reject_link(ancestor)?;
    }
    Ok(fs::canonicalize(absolute)?)
}
pub(crate) fn same_destination(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    matches!((destination(a), destination(b)), (Ok(a), Ok(b)) if a == b)
}
pub(crate) fn other_receipts(root: &Path, receipt: &Receipt) -> Result<Vec<Receipt>> {
    Ok(receipts(root)?
        .into_iter()
        .filter(|other| {
            other.id != receipt.id && same_destination(&other.destination, &receipt.destination)
        })
        .collect())
}
pub(crate) fn current(root: &Path, relative: &str) -> Result<Option<Vec<u8>>> {
    let path = validation::contained(root, relative)?;
    if path.exists() {
        Ok(Some(storage::bytes(&path)?))
    } else {
        Ok(None)
    }
}
pub(crate) fn file_mode(path: &Path) -> Result<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(fs::metadata(path)?.permissions().mode() & 0o7777)
    }
    #[cfg(not(unix))]
    {
        let _ = fs::metadata(path)?;
        Ok(0o644)
    }
}
pub(crate) fn check_mode(path: &Path, expected: Option<&u32>) -> Result<()> {
    #[cfg(unix)]
    if let Some(mode) = expected
        && file_mode(path)? != *mode
    {
        return Err(Error::new(
            "mode-conflict",
            "Owned file permissions changed by user",
        ));
    }
    let _ = (path, expected);
    Ok(())
}
pub(crate) fn enabled_configs<'a>(others: &'a [Receipt], path: &str) -> Vec<&'a OwnedConfig> {
    others
        .iter()
        .filter(|r| r.enabled)
        .filter_map(|r| r.configs.get(path))
        .collect()
}
pub(crate) fn shared_file(
    others: &[Receipt],
    path: &str,
    digest: &str,
    mode: Option<&u32>,
    enabled_only: bool,
) -> Result<bool> {
    let mut shared = false;
    for owner in others.iter().filter(|r| !enabled_only || r.enabled) {
        if owner.configs.contains_key(path) {
            return Err(Error::new(
                "ownership-conflict",
                "Raw file overlaps a managed configuration",
            ));
        }
        if let Some(owned) = owner.files.get(path) {
            if owned != digest || owner.modes.get(path).zip(mode).is_some_and(|(a, b)| a != b) {
                return Err(Error::new(
                    "ownership-conflict",
                    format!("Other package owns incompatible file: {path}"),
                ));
            }
            shared = true;
        }
    }
    Ok(shared)
}

/// Existing callers retain project scope. A user scope always receives an explicit home root.
pub fn install(
    root: &Path,
    id: &str,
    target: &str,
    destination: &Path,
    dry: bool,
) -> Result<Value> {
    install_scoped(root, id, target, destination, Scope::Project, dry)
}
pub fn install_scoped(
    root: &Path,
    id: &str,
    target: &str,
    selected: &Path,
    scope: Scope,
    dry: bool,
) -> Result<Value> {
    install_scoped_approved(root, id, target, selected, scope, dry, None)
}

pub fn install_scoped_approved(
    root: &Path,
    id: &str,
    target: &str,
    selected: &Path,
    scope: Scope,
    dry: bool,
    native_approval: Option<&str>,
) -> Result<Value> {
    let accepted = crate::collection_skills::find(root, id)?;
    let required = accepted
        .as_ref()
        .filter(|a| crate::collections::native::required(a));
    if let Some(a) = required {
        crate::collections::native::assess(a, target, Some(scope))?;
    }
    let approval = required
        .map(|a| storage::encoded(a).map(|b| storage::digest(&b)))
        .transpose()?;
    if !dry
        && approval
            .as_deref()
            .is_some_and(|digest| native_approval != Some(digest))
    {
        return Err(Error::new(
            "native-approval-required",
            "Preview and explicitly approve this exact native package digest before enabling project hooks",
        ));
    }
    let mut candidate = planning::candidate(root, id, target, selected, scope)?;
    let destination = candidate.receipt.destination.clone();
    let identity = storage::digest(&storage::encoded(&(id, target, &destination, scope))?);
    let receipt_id = format!("{}-{}", &id[..id.len().min(35)], &identity[..20]);
    let all = receipts(root)?;
    let previous = all
        .iter()
        .find(|r| r.id == receipt_id)
        .or_else(|| {
            all.iter().find(|r| {
                r.package == id
                    && r.target == target
                    && r.scope == scope
                    && same_destination(&r.destination, &destination)
            })
        })
        .cloned();
    if previous.as_ref().is_some_and(|r| !r.enabled) {
        return Err(Error::new(
            "disabled",
            "Enable this receipt before updating its installed payload",
        ));
    }
    candidate.receipt.id = previous.as_ref().map_or(receipt_id, |r| r.id.clone());
    let others = all
        .into_iter()
        .filter(|r| r.id != candidate.receipt.id && same_destination(&r.destination, &destination))
        .collect::<Vec<_>>();
    let (receipt, changes) = planning::reconcile(candidate, previous.as_ref(), &others)?;
    if dry {
        return Ok(
            json!({"dry_run":true,"scope":scope,"destination":receipt.destination,"files":receipt.files.keys().collect::<Vec<_>>(),"configs":receipt.configs.keys().collect::<Vec<_>>(),"changes":changes.len(),"delivery":receipt.delivery,"native_approval":approval}),
        );
    }
    commit_receipt(root, &receipt, changes)?;
    Ok(serde_json::to_value(receipt)?)
}

pub(crate) fn commit_receipt(root: &Path, receipt: &Receipt, changes: Vec<Change>) -> Result<()> {
    let path = receipt_path(root, &receipt.id)?;
    let before = if path.exists() {
        Some(storage::bytes(&path)?)
    } else {
        None
    };
    transaction::apply_with_receipt_modes(
        root,
        &receipt.destination,
        &receipt.operation,
        changes,
        Change {
            path: format!(".forge-state/receipts/{}.json", receipt.id),
            before,
            after: Some(storage::encoded(receipt)?),
        },
        &receipt.modes,
    )?;
    Ok(())
}
