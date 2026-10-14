use crate::contract::Result;
use chrono::Utc;
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub fn reason(error: &(dyn std::error::Error + 'static)) -> String {
    let text = error.to_string();
    if text.contains("timeout") {
        "deadline-exceeded"
    } else if text.contains("cancelled") {
        "cancelled"
    } else if text.contains("identity") || text.contains("SHA") {
        "identity-refused"
    } else if text.contains("input") || text.contains("manifest") {
        "input-refused"
    } else if text.contains("control") || text.contains("Docker") || text.contains("container") {
        "control-unavailable"
    } else if text.contains("output") || text.contains("Evidence") {
        "publication-or-output-failed"
    } else {
        "operation-failed"
    }
    .into()
}
pub struct Log {
    file: Option<File>,
}
impl Log {
    pub fn new(directory: &Path) -> Self {
        let name = format!(
            "forge-ci-input_{}_UTC.log",
            Utc::now().format("%Y-%m-%d_%H-%M-%S")
        );
        let file = (0..100).find_map(|suffix| {
            let path = if suffix == 0 {
                directory.join(&name)
            } else {
                directory.join(name.replace("_UTC.log", &format!("_UTC-{suffix}.log")))
            };
            match OpenOptions::new().create_new(true).write(true).open(path) {
                Ok(file) => Some(file),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(_) => None,
            }
        });
        if file.is_none() {
            eprintln!("Diagnostic log initialization failed; console fallback active");
        }
        Self { file }
    }
    pub fn event(&mut self, level: &str, component: &str, message: &str) {
        let line = format!(
            "[{} UTC] [{level}] [{component}] {message}\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S")
        );
        if let Some(file) = self.file.as_mut() {
            if file
                .write_all(line.as_bytes())
                .and_then(|_| file.flush())
                .is_err()
            {
                self.file = None;
                eprintln!("Diagnostic logging failed; console fallback active");
            }
        } else {
            eprint!("{line}");
        }
    }
}
#[derive(Serialize)]
pub struct Evidence {
    pub schema: u32,
    pub run_id: String,
    pub contract: &'static str,
    pub reason: String,
    pub source_revision: String,
    pub image: String,
    pub manifest_digest: String,
    pub mode: String,
    pub outcome: String,
    pub exit: Option<i32>,
    pub cleanup_verified: bool,
    pub elapsed_ms: u128,
    pub worker: Option<serde_json::Value>,
    pub exclusions: Vec<&'static str>,
}
pub fn publish(directory: &Path, value: &impl Serialize) -> Result<PathBuf> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > 1024 * 1024 {
        return Err("Evidence exceeds 1 MiB".into());
    }
    let temporary = directory.join("result.pending");
    let final_path = directory.join("result.json");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    // Atomic no-replace publication on the same filesystem, never rename over old evidence.
    fs::hard_link(&temporary, &final_path)?;
    fs::remove_file(&temporary)?;
    Ok(final_path)
}
