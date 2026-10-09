use crate::{Error, Result, storage, validation};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

pub struct Writer {
    _file: File,
}
impl Writer {
    pub fn acquire(root: &Path) -> Result<Self> {
        let directory = root.join(".forge-state");
        fs::create_dir_all(&directory)?;
        validation::reject_link(&directory)?;
        let path = directory.join("writer.lock");
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.try_lock()
            .map_err(|_| Error::new("busy", "Another Forge operation owns this workspace"))?;
        Ok(Self { _file: file })
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Change {
    pub path: String,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Journal {
    pub id: String,
    pub destination: PathBuf,
    pub state: String,
    pub changes: Vec<Change>,
}

// Legacy inline journals remain readable; new journals keep bounded metadata and raw blobs.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Payload {
    Blob { digest: String },
    Inline(Vec<u8>),
}
#[derive(Serialize, Deserialize)]
struct StoredChange {
    #[serde(default)]
    before_mode: Option<u32>,
    #[serde(default)]
    after_mode: Option<u32>,
    path: String,
    before: Option<Payload>,
    after: Option<Payload>,
}
#[derive(Serialize, Deserialize)]
struct StoredJournal {
    id: String,
    destination: PathBuf,
    state: String,
    changes: Vec<StoredChange>,
    #[serde(default)]
    receipt: Option<StoredChange>,
}

pub fn operation_id() -> String {
    format!(
        "op-{}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        std::process::id()
    )
}
fn journal_path(workspace: &Path, id: &str) -> Result<PathBuf> {
    validation::identifier(id)?;
    Ok(workspace
        .join(".forge-state/history")
        .join(format!("{id}.json")))
}
fn blob_path(workspace: &Path, digest: &str) -> Result<PathBuf> {
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::new("integrity", "Invalid journal blob digest"));
    }
    let directory = workspace.join(".forge-state/history-blobs");
    fs::create_dir_all(&directory)?;
    validation::contained(&directory, digest)
}
fn store_payload(workspace: &Path, bytes: Option<Vec<u8>>) -> Result<Option<Payload>> {
    let Some(bytes) = bytes else { return Ok(None) };
    if bytes.len() as u64 > storage::MAX_FILE {
        return Err(Error::new(
            "size-limit",
            "Journal payload exceeds the file limit",
        ));
    }
    let digest = storage::digest(&bytes);
    let path = blob_path(workspace, &digest)?;
    if path.exists() {
        if storage::digest(&storage::bytes(&path)?) != digest {
            return Err(Error::new("integrity", "Journal blob digest mismatch"));
        }
    } else {
        storage::atomic(&path, &bytes)?;
    }
    Ok(Some(Payload::Blob { digest }))
}
fn load_payload(workspace: &Path, payload: &Option<Payload>) -> Result<Option<Vec<u8>>> {
    Ok(match payload {
        None => None,
        Some(Payload::Inline(bytes)) => Some(bytes.clone()),
        Some(Payload::Blob { digest }) => {
            let bytes = storage::bytes(&blob_path(workspace, digest)?)?;
            if storage::digest(&bytes) != *digest {
                return Err(Error::new("integrity", "Journal blob digest mismatch"));
            }
            Some(bytes)
        }
    })
}
fn store_change(workspace: &Path, change: Change) -> Result<StoredChange> {
    Ok(StoredChange {
        before_mode: None,
        after_mode: None,
        path: change.path,
        before: store_payload(workspace, change.before)?,
        after: store_payload(workspace, change.after)?,
    })
}
fn load_change(workspace: &Path, change: &StoredChange) -> Result<Change> {
    Ok(Change {
        path: change.path.clone(),
        before: load_payload(workspace, &change.before)?,
        after: load_payload(workspace, &change.after)?,
    })
}

pub fn apply(workspace: &Path, destination: &Path, changes: Vec<Change>) -> Result<String> {
    apply_operation(workspace, destination, &operation_id(), changes, None, &[])
}
pub fn apply_with_receipt(
    workspace: &Path,
    destination: &Path,
    id: &str,
    changes: Vec<Change>,
    receipt: Change,
) -> Result<String> {
    apply_operation(workspace, destination, id, changes, Some(receipt), &[])
}
pub fn apply_modes(
    workspace: &Path,
    destination: &Path,
    changes: Vec<Change>,
    modes: &[(String, Option<u32>, Option<u32>)],
) -> Result<String> {
    apply_operation(
        workspace,
        destination,
        &operation_id(),
        changes,
        None,
        modes,
    )
}
fn apply_operation(
    workspace: &Path,
    destination: &Path,
    id: &str,
    changes: Vec<Change>,
    receipt: Option<Change>,
    modes: &[(String, Option<u32>, Option<u32>)],
) -> Result<String> {
    validation::reject_link(destination)?;
    let path = journal_path(workspace, id)?;
    if path.exists() {
        return Err(Error::new(
            "journal-conflict",
            "Operation identity already exists",
        ));
    }
    if changes.len() > storage::MAX_FILES {
        return Err(Error::new(
            "size-limit",
            "Journal has too many file changes",
        ));
    }
    let mut total = 0u64;
    for c in changes.iter().chain(receipt.iter()) {
        for bytes in c.before.iter().chain(c.after.iter()) {
            if bytes.len() as u64 > storage::MAX_FILE {
                return Err(Error::new(
                    "size-limit",
                    "Journal payload exceeds the file limit",
                ));
            }
            total += bytes.len() as u64;
        }
    }
    if total > storage::MAX_TOTAL * 2 + storage::MAX_FILE * 2 {
        return Err(Error::new(
            "size-limit",
            "Journal exceeds the operation byte limit",
        ));
    }
    for c in &changes {
        compare(destination, c, true)?;
    }
    if let Some(c) = &receipt {
        compare(workspace, c, true)?;
    }
    let mut journal = StoredJournal {
        id: id.into(),
        destination: destination.to_path_buf(),
        state: "prepared".into(),
        changes: changes
            .into_iter()
            .map(|c| store_change(workspace, c))
            .collect::<Result<_>>()?,
        receipt: receipt.map(|c| store_change(workspace, c)).transpose()?,
    };
    for (name, before, after) in modes {
        let change = journal
            .changes
            .iter_mut()
            .find(|c| &c.path == name)
            .ok_or_else(|| Error::new("mode", "Mode change requires journal payload"))?;
        change.before_mode = *before;
        change.after_mode = *after;
    }
    // Reserve the longest state name before any destination changes are possible.
    journal.state = "recovery-required".into();
    if storage::encoded(&journal)?.len() as u64 > storage::MAX_FILE {
        return Err(Error::new(
            "size-limit",
            "Journal metadata exceeds the read limit",
        ));
    }
    journal.state = "prepared".into();
    storage::write(&path, &journal)?;
    journal.state = "applying".into();
    storage::write(&path, &journal)?;
    let outcome = (|| -> Result<()> {
        for change in &journal.changes {
            set_stored(workspace, destination, change, true)?;
        }
        if let Some(change) = &journal.receipt {
            set_stored(workspace, workspace, change, true)?;
        }
        Ok(())
    })();
    if let Err(error) = outcome {
        journal.state = "recovery-required".into();
        storage::write(&path, &journal)?;
        return Err(Error::new(
            "recovery-required",
            format!("Operation {id} interrupted: {}", error.code),
        ));
    }
    journal.state = "committed".into();
    storage::write(&path, &journal)?;
    Ok(id.into())
}
fn current(root: &Path, path: &str) -> Result<Option<Vec<u8>>> {
    let path = validation::contained(root, path)?;
    if path.exists() {
        Ok(Some(storage::bytes(&path)?))
    } else {
        Ok(None)
    }
}
fn compare(root: &Path, c: &Change, forward: bool) -> Result<()> {
    let expected = if forward { &c.before } else { &c.after };
    if &current(root, &c.path)? != expected {
        return Err(Error::new(
            "conflict",
            format!("File changed concurrently: {}", c.path),
        ));
    }
    Ok(())
}
fn set(root: &Path, c: &Change, forward: bool) -> Result<()> {
    compare(root, c, forward)?;
    let path = validation::contained(root, &c.path)?;
    match if forward { &c.after } else { &c.before } {
        Some(bytes) => storage::atomic(&path, bytes),
        None => {
            if path.exists() {
                fs::remove_file(path)?;
            }
            Ok(())
        }
    }
}
fn mode_matches(root: &Path, relative: &str, expected: Option<u32>) -> Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Some(mode) = expected {
            let path = validation::contained(root, relative)?;
            return Ok(!path.exists() || fs::metadata(path)?.permissions().mode() & 0o777 == mode);
        }
    }
    let _ = (root, relative, expected);
    Ok(true)
}
fn set_stored(workspace: &Path, root: &Path, stored: &StoredChange, forward: bool) -> Result<()> {
    let c = load_change(workspace, stored)?;
    let expected_mode = if forward {
        stored.before_mode
    } else {
        stored.after_mode
    };
    if !mode_matches(root, &c.path, expected_mode)?
        && !(!forward && mode_matches(root, &c.path, stored.before_mode)?)
    {
        return Err(Error::new(
            "mode-conflict",
            "Executable metadata changed concurrently",
        ));
    }
    set(root, &c, forward)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if forward {
            stored.after_mode
        } else {
            stored.before_mode
        };
        if let Some(mode) = mode {
            let path = validation::contained(root, &c.path)?;
            if path.exists() {
                fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
            }
        }
    }
    Ok(())
}
pub fn rollback(workspace: &Path, id: &str) -> Result<()> {
    let path = journal_path(workspace, id)?;
    let mut journal: StoredJournal = storage::read(&path)?;
    if journal.state == "rolled-back" {
        return Ok(());
    }
    if journal.state != "committed" {
        return Err(Error::new(
            "journal-state",
            "Recover an interrupted operation before rollback",
        ));
    }
    for change in &journal.changes {
        compare(
            &journal.destination,
            &load_change(workspace, change)?,
            false,
        )?;
    }
    if let Some(change) = &journal.receipt {
        compare(workspace, &load_change(workspace, change)?, false)?;
    }
    // Record the direction before the first compensation so a crash resumes rollback.
    journal.state = "rolling-back".into();
    storage::write(&path, &journal)?;
    compensate(workspace, &journal)?;
    journal.state = "rolled-back".into();
    storage::write(&path, &journal)
}
fn compensate(workspace: &Path, journal: &StoredJournal) -> Result<()> {
    // Validate all entries before changing ownership or payloads; load one blob pair at a time.
    for (root, stored) in journal.receipt.iter().map(|c| (workspace, c)).chain(
        journal
            .changes
            .iter()
            .map(|c| (journal.destination.as_path(), c)),
    ) {
        let c = load_change(workspace, stored)?;
        let bytes = current(root, &c.path)?;
        if bytes != c.before && bytes != c.after {
            return Err(Error::new(
                "recovery-conflict",
                format!("User edit prevents recovery of {}", c.path),
            ));
        }
    }
    for (root, stored) in journal.receipt.iter().map(|c| (workspace, c)).chain(
        journal
            .changes
            .iter()
            .rev()
            .map(|c| (journal.destination.as_path(), c)),
    ) {
        let c = load_change(workspace, stored)?;
        if current(root, &c.path)? != c.before || !mode_matches(root, &c.path, stored.before_mode)?
        {
            set_stored(workspace, root, stored, false)?;
        }
    }
    Ok(())
}
pub fn recover(workspace: &Path) -> Result<()> {
    let directory = workspace.join(".forge-state/history");
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let mut journal: StoredJournal = storage::read(&path)?;
        if matches!(
            journal.state.as_str(),
            "prepared" | "applying" | "recovery-required" | "rolling-back"
        ) {
            compensate(workspace, &journal)?;
            journal.state = "rolled-back".into();
            storage::write(&path, &journal)?;
        }
    }
    Ok(())
}
