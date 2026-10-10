#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use forge_core::logging::{RunLog, redact};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct DesktopLog(Mutex<Option<(PathBuf, Arc<RunLog>)>>);

#[tauri::command]
async fn dispatch(
    root: String,
    request: Value,
    logs: tauri::State<'_, DesktopLog>,
) -> Result<Value, String> {
    let root = PathBuf::from(root);
    if !root.is_absolute() || !root.is_dir() {
        return Err("workspace: Select an absolute path to an existing workspace folder".into());
    }
    forge_core::validation::reject_link(&root).map_err(|error| redact(&error.to_string()))?;
    let action = request
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| "input: Supply an operation action".to_string())?;
    let actions = [
        "status",
        "source-list",
        "source-add",
        "discover",
        "approve",
        "catalog",
        "package-add",
        "harnesses",
        "compatibility",
        "sync",
        "build",
        "validate",
        "installed",
        "install",
        "disable",
        "enable",
        "uninstall",
        "rollback",
        "history",
        "logs",
        "doctor",
        "settings",
        "publish",
        "source-inspect",
        "source-policy",
        "package-inspect",
        "collection-sync",
        "collection-verify",
        "support-bundle",
        "log-retention",
        "job-run",
        "job-list",
        "job-status",
        "job-cancel",
        "job-retry",
        "update",
    ];
    if !actions.contains(&action) {
        return Err("action: Unknown desktop operation".into());
    }
    let action = action.to_owned();
    let log = {
        let mut current = logs
            .0
            .lock()
            .map_err(|_| "logging: Desktop log state is unavailable")?;
        if current.as_ref().is_none_or(|(path, _)| path != &root) {
            match RunLog::start("numera-forge-desktop", &root) {
                Ok(log) => *current = Some((root.clone(), Arc::new(log))),
                Err(error) => {
                    eprintln!("WARNING logging: {}", redact(&error.to_string()));
                    *current = None;
                }
            }
        }
        current.as_ref().map(|(_, log)| Arc::clone(log))
    };
    let join_log = log.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        if let Some(log) = &log {
            log.event("INFO", "dispatch", &format!("action={action} started"));
        }
        let result = forge_core::service::execute(&root, request);
        match &result {
            Ok(value) => {
                let status = value
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("response-received");
                let level = if matches!(
                    status,
                    "partial" | "conflict" | "failed" | "review-required"
                ) {
                    "WARNING"
                } else {
                    "INFO"
                };
                if let Some(log) = &log {
                    log.event(
                        level,
                        "dispatch",
                        &format!(
                            "action={action} result={status} duration_ms={}",
                            started.elapsed().as_millis()
                        ),
                    );
                }
            }
            Err(error) => {
                // Log the stable error code, never request fields or arbitrary response bodies.
                if let Some(log) = &log {
                    log.event(
                        "ERROR",
                        "dispatch",
                        &format!(
                            "action={action} code={} duration_ms={}",
                            error.code,
                            started.elapsed().as_millis()
                        ),
                    );
                }
            }
        }
        result.map_err(|error| redact(&error.to_string()))
    })
    .await
    .map_err(|error| {
        if let Some(log) = &join_log {
            log.event(
                "ERROR",
                "dispatch",
                "Blocking service task failed to complete",
            );
        }
        format!("desktop-task: {}", redact(&error.to_string()))
    })?
}

fn main() {
    if let Err(error) = tauri::Builder::default()
        .manage(DesktopLog::default())
        .invoke_handler(tauri::generate_handler![dispatch])
        .run(tauri::generate_context!())
    {
        eprintln!("Desktop startup failed: {}", redact(&error.to_string()));
        std::process::exit(1);
    }
}
