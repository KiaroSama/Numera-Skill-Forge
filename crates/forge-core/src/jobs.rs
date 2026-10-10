use crate::{Error, Result, storage, transaction, validation};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

thread_local! { static CONTEXT: RefCell<Option<(PathBuf, Instant)>> = const { RefCell::new(None) }; }
struct Context;
impl Drop for Context {
    fn drop(&mut self) {
        CONTEXT.with(|context| *context.borrow_mut() = None);
    }
}
pub fn checkpoint() -> Result<()> {
    CONTEXT.with(|context| {
        if let Some((cancel, deadline)) = &*context.borrow() {
            if cancel.exists() {
                return Err(Error::new(
                    "cancelled",
                    "Operation cancelled; committed transactions remain coherent",
                ));
            }
            if Instant::now() >= *deadline {
                return Err(Error::new(
                    "timeout",
                    "Operation exceeded its requested deadline",
                ));
            }
        }
        Ok(())
    })
}
fn directory(root: &Path) -> Result<PathBuf> {
    validation::reject_link(root)?;
    for ancestor in root.ancestors() {
        validation::reject_link(ancestor)?;
    }
    let state = root.join(".forge-state");
    if state.exists() {
        validation::reject_link(&state)?;
    }
    fs::create_dir_all(&state)?;
    let path = validation::contained(&state, "jobs")?;
    fs::create_dir_all(&path)?;
    Ok(path)
}
fn record_path(root: &Path, id: &str) -> Result<PathBuf> {
    validation::identifier(id)?;
    validation::contained(&directory(root)?, &format!("{id}.json"))
}
fn owner(root: &Path, id: &str) -> Result<File> {
    validation::identifier(id)?;
    let path = validation::contained(&directory(root)?, &format!("{id}.owner"))?;
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?)
}
pub fn status(root: &Path, id: &str) -> Result<Value> {
    let mut record: Value = storage::read(&record_path(root, id)?)?;
    if record["status"] == "running" && owner(root, id)?.try_lock().is_ok() {
        record["status"] = json!("interrupted");
        record["message"] = json!("Owner exited; inspect recovery and retry explicitly");
    }
    Ok(record)
}
pub fn list(root: &Path) -> Result<Value> {
    let mut records = vec![];
    for entry in fs::read_dir(directory(root)?)? {
        let path = entry?.path();
        if path.extension().and_then(|v| v.to_str()) == Some("json") {
            let id = path
                .file_stem()
                .and_then(|v| v.to_str())
                .ok_or_else(|| Error::new("encoding", "Invalid job record name"))?;
            records.push(status(root, id)?);
        }
    }
    records.sort_by(|a, b| a["started_utc"].as_str().cmp(&b["started_utc"].as_str()));
    Ok(json!(records))
}
pub fn cancel(root: &Path, id: &str) -> Result<Value> {
    let record = status(root, id)?;
    if record["status"] != "running" {
        return Ok(json!({"job":id,"status":record["status"],"cancel_requested":false}));
    }
    storage::atomic(
        &validation::contained(&directory(root)?, &format!("{id}.cancel"))?,
        b"cancel\n",
    )?;
    Ok(json!({"job":id,"cancel_requested":true,"status":"cancelling"}))
}
pub fn retry(root: &Path, id: &str) -> Result<Value> {
    let previous = status(root, id)?;
    if !matches!(
        previous["status"].as_str(),
        Some("failed" | "cancelled" | "interrupted" | "partial")
    ) {
        return Err(Error::new(
            "job-state",
            "Only failed, cancelled, interrupted or partial jobs can be retried",
        ));
    }
    run(
        root,
        previous["request"].clone(),
        previous["timeout"].as_u64().unwrap_or(120),
        None,
    )
}
pub fn run(root: &Path, request: Value, seconds: u64, requested_id: Option<&str>) -> Result<Value> {
    if !(1..=3600).contains(&seconds) {
        return Err(Error::new("timeout", "Job timeout must be1-3600seconds"));
    }
    let action = request["action"]
        .as_str()
        .ok_or_else(|| Error::new("input", "Supply job operation"))?;
    if !matches!(
        action,
        "sync" | "discover" | "build" | "collection-sync" | "collection-verify" | "update"
    ) {
        return Err(Error::new(
            "job-action",
            "Only bounded source/build/installed operations are job-enabled",
        ));
    }
    let allowed = ["action", "source", "package", "target", "apply", "offline"];
    if !request
        .as_object()
        .is_some_and(|map| map.keys().all(|key| allowed.contains(&key.as_str())))
    {
        return Err(Error::new(
            "input",
            "Job request contains unsupported fields",
        ));
    }
    for key in ["source", "package", "target"] {
        if let Some(value) = request.get(key) {
            let value = value
                .as_str()
                .ok_or_else(|| Error::new("input", "Job identifiers must be strings"))?;
            validation::identifier(value)?;
        }
    }
    for key in ["apply", "offline"] {
        if request.get(key).is_some_and(|value| !value.is_boolean()) {
            return Err(Error::new("input", "Job flags must be boolean"));
        }
    }
    for key in match action {
        "discover" => &["source"][..],
        "build" => &["package", "target"][..],
        _ => &[][..],
    } {
        if !request.get(*key).is_some_and(Value::is_string) {
            return Err(Error::new("input", "Required job identifier is missing"));
        }
    }
    let id = requested_id
        .map(str::to_owned)
        .unwrap_or_else(transaction::operation_id);
    validation::identifier(&id)?;
    let path = record_path(root, &id)?;
    let lock = owner(root, &id)?;
    lock.try_lock()
        .map_err(|_| Error::new("job-running", "Selected job has an active owner"))?;
    if path.exists() {
        return Err(Error::new("job-identity", "Job identity already exists"));
    }
    let cancel = validation::contained(&directory(root)?, &format!("{id}.cancel"))?;
    CONTEXT.with(|context| {
        *context.borrow_mut() = Some((cancel, Instant::now() + Duration::from_secs(seconds)))
    });
    let _context = Context;
    let mut record = json!({"id":id,"request":request,"timeout":seconds,"started_utc":chrono::Utc::now().to_rfc3339(),"status":"running"});
    storage::write(&path, &record)?;
    let result = crate::service::execute(root, request);
    record["ended_utc"] = json!(chrono::Utc::now().to_rfc3339());
    match result {
        Ok(value) => {
            record["status"] = json!(if value["status"] == "partial" {
                "partial"
            } else {
                "success"
            });
            record["result"] = value;
        }
        Err(error) => {
            record["status"] = json!(if error.code == "cancelled" {
                "cancelled"
            } else {
                "failed"
            });
            record["error"] =
                json!({"code":error.code,"message":crate::logging::redact(&error.message)});
        }
    }
    if storage::encoded(&record)?.len() as u64 > storage::MAX_FILE {
        record["result"] = json!({"status":"result-omitted","reason":"Result exceeds durable job record limit; inspect the operation-specific stored artifact"});
    }
    storage::write(&path, &record)?;
    Ok(record)
}
