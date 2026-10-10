use crate::{Error, Result};
use chrono::Utc;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub struct RunLog {
    file: Mutex<File>,
    pub path: PathBuf,
    pub run_id: String,
}
impl RunLog {
    pub fn start(name: &str, root: &Path) -> Result<Self> {
        crate::validation::identifier(name)?;
        for ancestor in root.ancestors() {
            crate::validation::reject_link(ancestor)?;
        }
        let directory = crate::validation::contained(root, ".forge-state/logs")?;
        fs::create_dir_all(&directory)?;
        let stamp = Utc::now().format("%Y-%m-%d_%H-%M-%S_UTC").to_string();
        for index in 0..1000 {
            let suffix = if index == 0 {
                String::new()
            } else {
                format!("_{index}")
            };
            let path = directory.join(format!("{name}_{stamp}{suffix}.log"));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    let log = Self {
                        file: Mutex::new(file),
                        path,
                        run_id: format!("{stamp}{suffix}"),
                    };
                    log.event(
                        "INFO",
                        "startup",
                        &format!(
                            "version={} platform={} run_id={}",
                            env!("CARGO_PKG_VERSION"),
                            std::env::consts::OS,
                            log.run_id
                        ),
                    );
                    return Ok(log);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(Error::new("logging", "Cannot allocate a unique run log"))
    }
    pub fn event(&self, level: &str, category: &str, message: &str) {
        if level == "DEBUG" && std::env::var("NUMERA_DEBUG").as_deref() != Ok("1") {
            return;
        }
        let safe = redact(message);
        if let Ok(mut file) = self.file.lock()
            && writeln!(
                file,
                "[{}] [{}] [{}] {}",
                Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                level,
                category,
                safe
            )
            .and_then(|_| file.flush())
            .is_err()
        {
            eprintln!("WARNING logging: failed to write diagnostic file");
        }
    }
}
impl Drop for RunLog {
    fn drop(&mut self) {
        self.event("INFO", "shutdown", "Run completed; log closed");
    }
}

pub fn redact(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if [
        "token",
        "password",
        "secret",
        "api_key",
        "apikey",
        "authorization",
        "cookie",
        "private_key",
    ]
    .iter()
    .any(|key| lower.contains(key))
    {
        return "[REDACTED SENSITIVE ENTRY]".into();
    }
    value
        .split_whitespace()
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            if lower.contains("token=")
                || lower.contains("password=")
                || lower.contains("secret=")
                || lower.starts_with("ghp_")
                || lower.starts_with("gho_")
                || lower.contains("github_pat_")
                || lower.contains("ghp_")
                || lower.contains("gho_")
                || lower.starts_with("sk-")
                || (word.contains("://") && word.contains('@'))
            {
                "[REDACTED]".to_owned()
            } else {
                word.replace(['\r', '\n'], " ")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
