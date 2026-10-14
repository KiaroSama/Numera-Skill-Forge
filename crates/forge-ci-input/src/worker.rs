use crate::{
    contract::{self, Manifest, Result},
    owner, projection,
};
use serde_json::json;
use std::{
    fs,
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    time::{Duration, Instant},
};

fn boundary() -> Result<()> {
    for (file, expected) in [
        ("memory.max", "4294967296"),
        ("memory.swap.max", "0"),
        ("pids.max", "256"),
    ] {
        if fs::read_to_string(format!("/sys/fs/cgroup/{file}"))?.trim() != expected {
            return Err("Effective resource limit unavailable".into());
        }
    }
    let cpu = fs::read_to_string("/sys/fs/cgroup/cpu.max")?;
    let values: Vec<_> = cpu.split_whitespace().collect();
    if values.len() != 2
        || values[0].parse::<u64>()?
            != values[1]
                .parse::<u64>()?
                .checked_mul(2)
                .ok_or("CPU limit overflow")?
    {
        return Err("Effective CPU limit unavailable".into());
    }
    let status = fs::read_to_string("/proc/self/status")?;
    if !status.lines().any(|l| l == "NoNewPrivs:\t1")
        || !status.lines().any(|l| l == "CapEff:\t0000000000000000")
        || !status.lines().any(|l| l == "Seccomp:\t2")
        || !status
            .lines()
            .any(|l| l.starts_with("Uid:\t1000\t1000\t1000\t1000"))
    {
        return Err("Worker privilege controls absent".into());
    }
    let names: Vec<_> = fs::read_dir("/sys/class/net")?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<std::io::Result<_>>()?;
    if names.len() != 1 || names[0] != "lo" {
        return Err("External network interface visible".into());
    }
    let address: SocketAddr = "198.51.100.1:80".parse()?;
    if TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok() {
        return Err("External network not denied".into());
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let stream = TcpStream::connect_timeout(&listener.local_addr()?, Duration::from_secs(1))?;
    drop(stream);
    drop(listener);
    #[cfg(unix)]
    for path in ["/input/control-write-probe", "/deps/control-write-probe"] {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(path)?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o777 != 0o666 {
            return Err("Read-only probe prerequisite absent".into());
        }
    }
    if fs::OpenOptions::new()
        .write(true)
        .open("/input/control-write-probe")
        .is_ok()
        || fs::OpenOptions::new()
            .write(true)
            .open("/deps/control-write-probe")
            .is_ok()
        || fs::read("/host-private-input-sentinel").is_ok()
        || Path::new("/var/run/docker.sock").exists()
    {
        return Err("Read-only or host-path control absent".into());
    }
    let allowed = [
        "PATH",
        "LANG",
        "LC_ALL",
        "HOME",
        "TMPDIR",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "CARGO_TARGET_DIR",
        "CARGO_BUILD_JOBS",
        "RUST_TEST_THREADS",
        "PLAYWRIGHT_BROWSERS_PATH",
        "CI",
        "CARGO_PROFILE_DEV_DEBUG",
    ];
    if std::env::vars_os().any(|(key, _)| key.to_str().is_none_or(|s| !allowed.contains(&s))) {
        return Err("Ambient environment exposed".into());
    }
    // Directory enumeration temporarily uses one descriptor; only standard pipes plus that descriptor are allowed.
    let descriptors: Vec<_> = fs::read_dir("/proc/self/fd")?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<std::io::Result<_>>()?;
    if descriptors.len() > 4 {
        return Err("Ambient file descriptor exposed".into());
    }
    Ok(())
}
fn product(
    program: &str,
    args: &[&str],
    cwd: &str,
    wall: u64,
    idle: u64,
) -> Result<serde_json::Value> {
    let mut command = owner::command(program);
    for key in [
        "PATH",
        "LANG",
        "LC_ALL",
        "HOME",
        "TMPDIR",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "CARGO_TARGET_DIR",
        "CARGO_BUILD_JOBS",
        "RUST_TEST_THREADS",
        "PLAYWRIGHT_BROWSERS_PATH",
        "CI",
        "CARGO_PROFILE_DEV_DEBUG",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.args(args).current_dir(cwd);
    let started = Instant::now();
    let result = owner::run(command, wall, idle)?;
    let record = json!({"argv":std::iter::once(program).chain(args.iter().copied()).collect::<Vec<_>>(),"exit":result.code,"stdout_digest":contract::digest(&result.stdout),"stderr_digest":result.stderr_digest,"elapsed_ms":started.elapsed().as_millis()});
    Ok(record)
}
pub fn execute(mode: &str) -> Result<()> {
    if !cfg!(target_os = "linux") {
        return Err("Linux worker required".into());
    }
    if !matches!(
        mode,
        "core" | "frontend" | "probe" | "timeout-probe" | "failure-probe"
    ) {
        return Err("Unsupported worker mode".into());
    }
    boundary()?;
    let manifest_bytes = fs::read("/input/manifest.json")?;
    if manifest_bytes.len() > 1024 * 1024 {
        return Err("Oversized input manifest".into());
    }
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)?;
    projection::verify(Path::new("/input"), &manifest)?;
    fs::create_dir_all("/scratch/home")?;
    fs::create_dir_all("/scratch/cargo")?;
    fs::write(
        "/scratch/cargo/config.toml",
        fs::read("/deps/cargo-config.toml")?,
    )?;
    let mut commands = Vec::new();
    let execution = (|| -> Result<()> {
        match mode {
            "core" => {
                commands.push(product(
                    "cargo",
                    &[
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
                    "/input",
                    480,
                    120,
                )?);
                if commands.last().is_some_and(|r| r["exit"] != 0) {
                    return Err("Selected product command failed".into());
                }
                commands.push(product(
                    "cargo",
                    &[
                        "build",
                        "--locked",
                        "--offline",
                        "-p",
                        "forge-cli",
                        "--release",
                    ],
                    "/input",
                    480,
                    120,
                )?);
            }
            "frontend" => {
                commands.push(product(
                    "npm",
                    &["run", "build"],
                    "/input/apps/desktop",
                    180,
                    45,
                )?);
                if commands.last().is_some_and(|r| r["exit"] != 0) {
                    return Err("Selected product command failed".into());
                }
                commands.push(product(
                    "npm",
                    &["run", "test:ui"],
                    "/input/apps/desktop",
                    120,
                    30,
                )?);
            }
            "timeout-probe" => loop {
                std::thread::park_timeout(Duration::from_secs(1));
            },
            "failure-probe" => {
                commands.push(product("/usr/bin/false", &[], "/input", 10, 5)?);
            }
            "probe" => {}
            _ => unreachable!(),
        }
        projection::verify(Path::new("/input"), &manifest)?;
        if commands.iter().any(|r| r["exit"] != 0) {
            return Err("Selected product command failed".into());
        }
        Ok(())
    })();
    let exit = if execution.is_ok() { 0 } else { 1 };
    println!(
        "FORGE_INPUT_RESULT {}",
        json!({"schema":1,"source_revision":manifest.source_revision,"manifest_digest":contract::digest(&manifest_bytes),"mode":mode,"controls_verified":true,"exit":exit,"commands":commands})
    );
    execution
}
