use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const MAX_SOURCE: u64 = 64 * 1024 * 1024;
pub const MAX_FILES: usize = 10_000;
pub const CONTRACT: &str = "forge-linux-input-v1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputFile {
    pub path: String,
    pub mode: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub contract: String,
    pub source_revision: String,
    pub files: Vec<InputFile>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerRecord {
    pub schema: u32,
    pub source_revision: String,
    pub manifest_digest: String,
    pub mode: String,
    pub controls_verified: bool,
    pub exit: i32,
    pub commands: Vec<CommandRecord>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandRecord {
    pub argv: Vec<String>,
    pub exit: i32,
    pub stdout_digest: String,
    pub stderr_digest: String,
    pub elapsed_ms: u128,
}
pub fn commands(mode: &str) -> Vec<Vec<String>> {
    let commands: Vec<Vec<&str>> = match mode {
        "core" => vec![
            vec![
                "cargo",
                "test",
                "--locked",
                "--offline",
                "-p",
                "forge-core",
                "-p",
                "forge-cli",
                "--",
                "--test-threads=2",
            ],
            vec![
                "cargo",
                "build",
                "--locked",
                "--offline",
                "-p",
                "forge-cli",
                "--release",
            ],
        ],
        "frontend" => vec![vec!["npm", "run", "build"], vec!["npm", "run", "test:ui"]],
        "failure-probe" => vec![vec!["/usr/bin/false"]],
        _ => vec![],
    };
    commands
        .into_iter()
        .map(|c| c.into_iter().map(str::to_owned).collect())
        .collect()
}
pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub fn revision(value: &str) -> Result<()> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("Source identity must be an exact lowercase Git SHA".into());
    }
    Ok(())
}
pub fn image(value: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err("Immutable image ID required".into());
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("Immutable image ID required".into());
    }
    Ok(())
}
pub fn selected(path: &str) -> bool {
    matches!(path, "Cargo.toml" | "Cargo.lock")
        || path.starts_with("crates/")
        || (path.starts_with("apps/desktop/") && !path.starts_with("apps/desktop/src-tauri/icons/"))
}
pub fn safe_path(path: &str) -> Result<()> {
    let forbidden = [
        ".git",
        ".ai",
        ".specify",
        "specs",
        "plans",
        ".claude",
        ".codex",
        ".kiro",
        ".cursor",
        ".agents",
        ".cline",
        "graphify-out",
        ".codebase-memory",
        ".ignoreme",
        "secrets.md",
        "explain-AI.md",
        "CLAUDE.md",
        "AGENTS.md",
        "reference.md",
        "node_modules",
        "target",
        "dist",
        "coverage",
        "logs",
    ];
    if path.is_empty()
        || path.len() > 1024
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.chars().any(char::is_control)
        || path.split('/').any(|s| {
            s.is_empty() || s == "." || s == ".." || forbidden.contains(&s) || s.starts_with(".env")
        })
    {
        return Err("Unsafe or protected input path".into());
    }
    Ok(())
}
pub fn validate(manifest: &Manifest) -> Result<()> {
    revision(&manifest.source_revision)?;
    if manifest.schema != 1
        || manifest.contract != CONTRACT
        || manifest.files.is_empty()
        || manifest.files.len() > MAX_FILES
    {
        return Err("Unsupported or oversized input manifest".into());
    }
    let mut previous = "";
    let mut size = 0u64;
    for file in &manifest.files {
        safe_path(&file.path)?;
        if !selected(&file.path)
            || file.path.as_str() <= previous
            || !matches!(file.mode.as_str(), "100644" | "100755")
            || file.sha256.len() != 64
            || !file
                .sha256
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err("Invalid or unordered input manifest".into());
        }
        previous = &file.path;
        size = size.checked_add(file.bytes).ok_or("Input size overflow")?;
    }
    if size > MAX_SOURCE {
        return Err("Source projection exceeds 64 MiB".into());
    }
    Ok(())
}
