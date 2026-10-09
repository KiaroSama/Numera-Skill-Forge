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

pub fn apply(workspace: &Path, destination: &Path, changes: Vec<Change>) -> Result<String> {
    validation::reject_link(destination)?;
    for c in &changes {
        compare(destination, c, true)?;
    }
    let id = format!(
        "op-{}-{}",
        chrono::Utc::now().timestamp_micros(),
        std::process::id()
    );
    let journal_path = workspace
        .join(".forge-state/history")
        .join(format!("{id}.json"));
    let mut journal = Journal {
        id: id.clone(),
        destination: destination.to_path_buf(),
        state: "prepared".into(),
        changes,
    };
    storage::write(&journal_path, &journal)?;
    journal.state = "applying".into();
    storage::write(&journal_path, &journal)?;
    for change in &journal.changes {
        if let Err(error) = set(destination, change, true) {
            journal.state = "recovery-required".into();
            storage::write(&journal_path, &journal)?;
            return Err(Error::new(
                "recovery-required",
                format!("Operation {id} interrupted: {}", error.code),
            ));
        }
    }
    journal.state = "committed".into();
    storage::write(&journal_path, &journal)?;
    Ok(id)
}
fn compare(root: &Path, c: &Change, forward: bool) -> Result<()> {
    let path = validation::contained(root, &c.path)?;
    let current = if path.exists() {
        Some(storage::bytes(&path)?)
    } else {
        None
    };
    let expected = if forward { &c.before } else { &c.after };
    if &current != expected {
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
pub fn rollback(workspace: &Path, id: &str) -> Result<()> {
    validation::identifier(id)?;
    let path = workspace
        .join(".forge-state/history")
        .join(format!("{id}.json"));
    let mut j: Journal = storage::read(&path)?;
    if j.state == "rolled-back" {
        return Ok(());
    }
    if j.state != "committed" {
        return Err(Error::new(
            "journal-state",
            "Recover an interrupted operation before rollback",
        ));
    }
    for c in &j.changes {
        compare(&j.destination, c, false)?;
    }
    for c in j.changes.iter().rev() {
        set(&j.destination, c, false)?;
    }
    j.state = "rolled-back".into();
    storage::write(&path, &j)
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
        let mut j: Journal = storage::read(&path)?;
        if matches!(
            j.state.as_str(),
            "prepared" | "applying" | "recovery-required"
        ) {
            for c in j.changes.iter().rev() {
                let p = validation::contained(&j.destination, &c.path)?;
                let current = if p.exists() {
                    Some(storage::bytes(&p)?)
                } else {
                    None
                };
                if current == c.before {
                    continue;
                }
                if current != c.after {
                    return Err(Error::new(
                        "recovery-conflict",
                        format!("User edit prevents recovery of {}", c.path),
                    ));
                }
                set(&j.destination, c, false)?;
            }
            j.state = "rolled-back".into();
            storage::write(&path, &j)?;
        }
    }
    Ok(())
}
