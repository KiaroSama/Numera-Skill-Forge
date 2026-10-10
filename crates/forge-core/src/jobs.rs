use crate::{Error, Result, storage, transaction, validation};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Step {
    kind: String,
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    status: String,
}
struct Active {
    cancel: PathBuf,
    deadline: Instant,
    record: PathBuf,
    selection: Option<Vec<Step>>,
}
thread_local! { static CONTEXT: RefCell<Option<Active>> = const { RefCell::new(None) }; }
struct Context;
impl Drop for Context {
    fn drop(&mut self) {
        CONTEXT.with(|context| *context.borrow_mut() = None);
    }
}
pub fn checkpoint() -> Result<()> {
    CONTEXT.with(|context| {
        if let Some(active) = &*context.borrow() {
            if active.cancel.exists() {
                return Err(Error::new(
                    "cancelled",
                    "Operation cancelled; committed transactions remain coherent",
                ));
            }
            if Instant::now() >= active.deadline {
                return Err(Error::new(
                    "timeout",
                    "Operation exceeded its requested deadline",
                ));
            }
        }
        Ok(())
    })
}
pub fn retrying() -> bool {
    CONTEXT.with(|context| {
        context
            .borrow()
            .as_ref()
            .is_some_and(|active| active.selection.is_some())
    })
}
pub fn selected(kind: &str, id: &str, target: Option<&str>) -> bool {
    CONTEXT.with(|context| {
        context
            .borrow()
            .as_ref()
            .and_then(|active| active.selection.as_ref())
            .is_none_or(|steps| {
                steps.iter().any(|step| {
                    step.kind == kind && step.id == id && step.target.as_deref() == target
                })
            })
    })
}
pub fn progress(kind: &str, id: &str, target: Option<&str>, status: &str) -> Result<()> {
    validation::identifier(id)?;
    if let Some(target) = target {
        validation::identifier(target)?;
    }
    CONTEXT.with(|context| {
        let borrowed = context.borrow();
        let Some(active) = borrowed.as_ref() else {
            return Ok(());
        };
        let mut record: Value = storage::read(&active.record)?;
        let steps = record["steps"]
            .as_array_mut()
            .ok_or_else(|| Error::new("job-record", "Job progress is invalid"))?;
        let step = serde_json::to_value(Step {
            kind: kind.into(),
            id: id.into(),
            target: target.map(str::to_owned),
            status: status.into(),
        })?;
        if let Some(existing) = steps
            .iter_mut()
            .find(|row| row["kind"] == kind && row["id"] == id && row["target"].as_str() == target)
        {
            *existing = step;
        } else {
            steps.push(step);
        }
        record["updated_utc"] = json!(chrono::Utc::now().to_rfc3339());
        storage::write(&active.record, &record)
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
    let selection = previous
        .get("steps")
        .map(|steps| -> Result<Vec<Step>> {
            let steps: Vec<Step> = serde_json::from_value(steps.clone())?;
            let mut retry = Vec::new();
            for mut step in steps {
                validation::identifier(&step.id)?;
                if let Some(target) = &step.target {
                    validation::identifier(target)?;
                }
                if !matches!(
                    step.kind.as_str(),
                    "source" | "build" | "receipt" | "collection"
                ) {
                    return Err(Error::new("job-record", "Unknown job progress kind"));
                }
                if matches!(step.status.as_str(), "failed" | "running" | "pending") {
                    step.status = "pending".into();
                    retry.push(step);
                }
            }
            Ok(retry)
        })
        .transpose()?;
    if selection.as_ref().is_some_and(Vec::is_empty)
        && matches!(
            previous["request"]["action"].as_str(),
            Some("sync" | "update" | "collection-sync")
        )
    {
        return Err(Error::new(
            "job-review",
            "No failed steps to retry; review-required content needs explicit approval",
        ));
    }
    let selection = selection.filter(|steps| !steps.is_empty());
    run_selected(
        root,
        previous["request"].clone(),
        previous["timeout"].as_u64().unwrap_or(120),
        None,
        selection,
        Some(id),
    )
}
pub fn run(root: &Path, request: Value, seconds: u64, requested_id: Option<&str>) -> Result<Value> {
    run_selected(root, request, seconds, requested_id, None, None)
}
fn run_selected(
    root: &Path,
    request: Value,
    seconds: u64,
    requested_id: Option<&str>,
    selection: Option<Vec<Step>>,
    retry_of: Option<&str>,
) -> Result<Value> {
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
    if CONTEXT.with(|context| context.borrow().is_some()) {
        return Err(Error::new("job-running", "Nested jobs are not supported"));
    }
    CONTEXT.with(|context| {
        *context.borrow_mut() = Some(Active {
            cancel,
            deadline: Instant::now() + Duration::from_secs(seconds),
            record: path.clone(),
            selection: selection.clone(),
        });
    });
    let _context = Context;
    let initial = json!({"id":id,"request":request,"timeout":seconds,"started_utc":chrono::Utc::now().to_rfc3339(),"status":"running","steps":selection.unwrap_or_default(),"retry_of":retry_of});
    storage::write(&path, &initial)?;
    let result = crate::service::execute(root, request);
    let mut record: Value = storage::read(&path)?;
    record["ended_utc"] = json!(chrono::Utc::now().to_rfc3339());
    match result {
        Ok(value) => {
            let unfinished = record["steps"].as_array().is_some_and(|steps| {
                steps.iter().any(|step| {
                    matches!(
                        step["status"].as_str(),
                        Some("pending" | "running" | "failed")
                    )
                })
            });
            record["status"] = json!(if value["status"] == "partial" || unfinished {
                "partial"
            } else {
                "success"
            });
            record["result"] = value;
            if unfinished && record["result"].is_object() {
                record["result"]["status"] = json!("partial");
            }
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
