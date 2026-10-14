use crate::{
    contract::{self, Result},
    evidence::{self, Evidence, Log},
    owner,
};
use serde_json::Value;
use std::{fs, path::Path, process::Command, time::Instant};

fn docker() -> Command {
    let mut command = owner::command("/usr/bin/docker");
    command.args(["--host", "unix:///var/run/docker.sock"]);
    command
}
fn query(args: &[&str]) -> Result<Value> {
    let mut command = docker();
    command.args(args);
    let result = owner::run(command, 20, 10)?;
    if result.code != 0 {
        return Err("Docker control query failed".into());
    }
    Ok(serde_json::from_slice(&result.stdout)?)
}
struct Container {
    id: String,
    name: String,
    removed: bool,
}
impl Container {
    fn remove(&mut self) -> Result<()> {
        if self.removed {
            return Ok(());
        }
        let selector = if self.id.is_empty() {
            self.name.as_str()
        } else {
            self.id.as_str()
        };
        let mut inspect = docker();
        inspect.args(["inspect", selector]);
        let result = owner::run(inspect, 20, 10)?;
        if result.code != 0 {
            let mut list = docker();
            list.args([
                "ps",
                "-aq",
                "--filter",
                &format!("label=numera.forge.input.owner={}", self.name),
            ]);
            let remaining = owner::run(list, 20, 10)?;
            if remaining.code == 0 && remaining.stdout.is_empty() {
                self.removed = true;
                return Ok(());
            }
            return Err("Owned container absence unverified".into());
        }
        let detail: Value = serde_json::from_slice(&result.stdout)?;
        if detail[0]["Config"]["Labels"]["numera.forge.input.owner"] != self.name {
            return Err("Container ownership mismatch".into());
        }
        let mut command = docker();
        command.args(["rm", "--force", selector]);
        if owner::run(command, 20, 10)?.code != 0 {
            return Err("Owned container cleanup failed".into());
        }
        let mut command = docker();
        command.args([
            "ps",
            "-aq",
            "--filter",
            &format!("label=numera.forge.input.owner={}", self.name),
        ]);
        let result = owner::run(command, 20, 10)?;
        if result.code != 0 || !result.stdout.is_empty() {
            return Err("Owned container survived cleanup".into());
        }
        self.removed = true;
        Ok(())
    }
}
impl Drop for Container {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

pub(crate) fn controls(detail: &Value, image: &str, name: &str) -> Result<()> {
    let config = &detail["Config"];
    let host = &detail["HostConfig"];
    let empty = |v: &Value| {
        v.is_null()
            || v.as_array().is_some_and(Vec::is_empty)
            || v.as_object().is_some_and(serde_json::Map::is_empty)
    };
    if detail["Image"] != image
        || config["User"] != "1000:1000"
        || config["OpenStdin"] != false
        || config["Tty"] != false
        || config["Labels"]["numera.forge.input.owner"] != name
        || host["NetworkMode"] != "none"
        || host["ReadonlyRootfs"] != true
        || host["Privileged"] != false
        || host["Memory"] != 4_294_967_296u64
        || host["MemorySwap"] != 4_294_967_296u64
        || host["NanoCpus"] != 2_000_000_000u64
        || host["PidsLimit"] != 256
        || !empty(&host["Binds"])
        || !empty(&host["Devices"])
        || !empty(&host["PortBindings"])
        || !empty(&host["CapAdd"])
        || host["PidMode"] != ""
        || host["IpcMode"] != "private"
        || host["UTSMode"] != ""
    {
        return Err("Required container controls missing".into());
    }
    let drops = host["CapDrop"]
        .as_array()
        .ok_or("Missing capability drop")?;
    let options = host["SecurityOpt"]
        .as_array()
        .ok_or("Missing no-new-privileges")?;
    if drops.len() != 1
        || drops[0]
            .as_str()
            .is_none_or(|s| !s.eq_ignore_ascii_case("all"))
        || options.len() != 1
        || !matches!(
            options[0].as_str(),
            Some("no-new-privileges" | "no-new-privileges:true")
        )
    {
        return Err("Unsafe capability or security configuration".into());
    }
    let mounts = detail["Mounts"]
        .as_array()
        .ok_or("Missing mount inspection")?;
    let tmpfs = host["Tmpfs"].as_object().ok_or("Missing private writes")?;
    if tmpfs.len() != WRITES.len()
        || mounts.iter().any(|m| {
            m["Type"] != "tmpfs"
                || m["Destination"]
                    .as_str()
                    .is_none_or(|s| !WRITES.contains(&s))
        })
    {
        return Err("Unexpected writable mount".into());
    }
    for path in WRITES {
        let options = tmpfs
            .get(path)
            .and_then(Value::as_str)
            .ok_or("Missing private write mount")?;
        if options != "rw,exec,nosuid,nodev,size=2147483648,mode=1777" {
            return Err("Unbounded private write mount".into());
        }
    }
    Ok(())
}
pub(crate) const WRITES: [&str; 8] = [
    "/scratch",
    "/tmp",
    "/input/.ci-work",
    "/input/.forge-state",
    "/input/apps/desktop/dist",
    "/input/apps/desktop/coverage",
    "/input/apps/desktop/node_modules/.vite",
    "/input/apps/desktop/node_modules/.vite-temp",
];

pub(crate) fn final_outcome(outcome: &str, cleanup: bool, cancelled: bool) -> &str {
    if !cleanup {
        "failed"
    } else if cancelled {
        "terminated"
    } else {
        outcome
    }
}

pub fn execute(image: &str, source: &str, mode: &str, output: &Path) -> Result<()> {
    let existed = output.exists();
    let result = execute_inner(image, source, mode, output);
    if let Err(error) = &result
        && !existed
        && output.is_dir()
        && !output.join("result.json").exists()
    {
        let record = Evidence {
            schema: 1,
            run_id: String::new(),
            contract: contract::CONTRACT,
            reason: evidence::reason(error.as_ref()),
            source_revision: source.into(),
            image: image.into(),
            manifest_digest: String::new(),
            mode: mode.into(),
            outcome: "refused".into(),
            exit: None,
            cleanup_verified: false,
            elapsed_ms: 0,
            worker: None,
            exclusions: vec!["product-not-verified", "historical-carryover"],
        };
        let _ = evidence::publish(output, &record);
    }
    result
}
fn execute_inner(image: &str, source: &str, mode: &str, output: &Path) -> Result<()> {
    if !cfg!(target_os = "linux") {
        return Err("Controlled job requires Linux".into());
    }
    contract::image(image)?;
    contract::revision(source)?;
    let (wall, idle) = match mode {
        "core" => (480, 120),
        "frontend" => (300, 45),
        "probe" | "failure-probe" => (60, 10),
        "timeout-probe" => (3, 2),
        _ => return Err("Unsupported controlled mode".into()),
    };
    fs::create_dir(output)?;
    let mut log = Log::new(output);
    log.event("INFO", "startup", "Controlled job requested");
    let started = Instant::now();
    let inspected = query(&["image", "inspect", image])?;
    let img = &inspected[0];
    if img["Id"] != image
        || img["Os"] != "linux"
        || img["Architecture"] != "amd64"
        || !img["Config"]["Volumes"].is_null()
        || img["Config"]["Labels"]["numera.forge.input.source"] != source
    {
        return Err("Image identity or declared source refused".into());
    }
    let random = fs::read_to_string("/proc/sys/kernel/random/uuid")?
        .trim()
        .replace('-', "");
    if random.len() != 32 || !random.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Run identity unavailable".into());
    }
    let name = format!("forge-input-{random}");
    log.event("INFO", "ownership", &format!("container={name}"));
    let mut create = docker();
    create.args([
        "create",
        "--pull=never",
        "--name",
        &name,
        "--label",
        &format!("numera.forge.input.owner={name}"),
        "--network=none",
        "--read-only",
        "--cap-drop=ALL",
        "--security-opt=no-new-privileges",
        "--ipc=private",
        "--pids-limit=256",
        "--memory=4g",
        "--memory-swap=4g",
        "--cpus=2",
        "--user=1000:1000",
        "--ulimit=nofile=1024:1024",
        "--log-driver=local",
        "--log-opt=max-size=16m",
        "--log-opt=max-file=1",
        "--log-opt=compress=false",
    ]);
    for path in WRITES {
        create.arg(format!(
            "--tmpfs={path}:rw,exec,nosuid,nodev,size=2147483648,mode=1777"
        ));
    }
    create.args(["--entrypoint=/usr/bin/sleep", image, "900"]);
    // Own the random labelled name BEFORE the daemon receives create, including uncertain responses.
    let mut container = Container {
        id: String::new(),
        name,
        removed: false,
    };
    let created = owner::run(create, 20, 10)?;
    if created.code != 0 {
        return Err("Restricted container creation failed".into());
    }
    let id = String::from_utf8(created.stdout)?.trim().to_owned();
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid container ID".into());
    }
    container.id = id;
    let mut outcome = "refused";
    let mut exit = None;
    let mut worker = None;
    let mut manifest_digest = img["Config"]["Labels"]["numera.forge.input.manifest"]
        .as_str()
        .ok_or("Missing image manifest identity")?
        .to_owned();
    if !contract::sha256(&manifest_digest) {
        return Err("Invalid image manifest identity".into());
    }
    let execution = (|| -> Result<()> {
        let detail = query(&["inspect", &container.id])?;
        controls(&detail[0], image, &container.name)?;
        log.event(
            "INFO",
            "preflight",
            "Actual container controls verified before startup",
        );
        let mut start = docker();
        start.args(["start", &container.id]);
        if owner::run(start, 20, 10)?.code != 0 {
            return Err("Container startup failed".into());
        }
        let mut exec = docker();
        exec.args([
            "exec",
            &container.id,
            "/usr/bin/env",
            "-i",
            "PATH=/usr/local/cargo/bin:/usr/local/bin:/usr/bin:/bin",
            "LANG=C.UTF-8",
            "LC_ALL=C.UTF-8",
            "HOME=/scratch/home",
            "TMPDIR=/tmp",
            "CARGO_HOME=/scratch/cargo",
            "RUSTUP_HOME=/usr/local/rustup",
            "CARGO_TARGET_DIR=/scratch/target",
            "CARGO_BUILD_JOBS=2",
            "RUST_TEST_THREADS=2",
            "CARGO_PROFILE_DEV_DEBUG=0",
            "PLAYWRIGHT_BROWSERS_PATH=/ms-playwright",
            "CI=1",
            "/usr/local/bin/forge-ci-input",
            "worker",
            mode,
        ]);
        outcome = "terminated";
        let result = owner::run(exec, wall, idle)?;
        outcome = "failed";
        exit = Some(result.code);
        log.event(
            "INFO",
            "execution",
            &format!(
                "exit={} stderr_digest={}",
                result.code, result.stderr_digest
            ),
        );
        let records: Vec<_> = result
            .stdout
            .split(|b| *b == b'\n')
            .filter_map(|l| l.strip_prefix(b"FORGE_INPUT_RESULT "))
            .collect();
        if records.len() != 1 {
            outcome = "failed";
            return Err("Missing or ambiguous worker evidence".into());
        }
        let typed: contract::WorkerRecord = serde_json::from_slice(records[0])?;
        let expected = contract::commands(mode);
        if typed.schema != 1
            || typed.source_revision != source
            || typed.mode != mode
            || !typed.controls_verified
            || (typed.exit == 0) != (result.code == 0)
            || typed.manifest_digest != manifest_digest
            || typed.manifest_digest.len() != 64
            || !typed
                .manifest_digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || typed.commands.len() > expected.len()
            || (result.code == 0 && typed.commands.len() != expected.len())
            || typed.commands.iter().zip(expected).any(|(actual, argv)| {
                actual.argv != argv
                    || (result.code == 0 && actual.exit != 0)
                    || !contract::sha256(&actual.stdout_digest)
                    || !contract::sha256(&actual.stderr_digest)
            })
        {
            outcome = "failed";
            return Err("Controlled worker evidence refused".into());
        }
        manifest_digest = typed.manifest_digest.clone();
        let value = serde_json::to_value(typed)?;
        worker = Some(value);
        if result.code != 0 {
            outcome = "failed";
            return Err("Selected product command failed".into());
        }
        if mode == "core" {
            let binary = output.join("numera-forge");
            let mut copy = docker();
            copy.args([
                "cp",
                &format!("{}:/scratch/target/release/numera-forge", container.id),
            ])
            .arg(&binary);
            if owner::run(copy, 20, 10)?.code != 0 {
                return Err("Verified CLI export failed".into());
            }
            let metadata = fs::symlink_metadata(&binary)?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > 64 * 1024 * 1024
            {
                return Err("Unsafe CLI export".into());
            }
        }
        outcome = "succeeded";
        Ok(())
    })();
    let cleanup = container.remove().is_ok();
    outcome = final_outcome(outcome, cleanup, owner::cancelled());
    let reason = if owner::cancelled() {
        "cancelled"
    } else if execution.is_err() {
        "execution-or-evidence-failed"
    } else if !cleanup {
        "cleanup-failed"
    } else {
        "complete"
    };
    let record = Evidence {
        schema: 1,
        run_id: container.name.clone(),
        reason: reason.into(),
        contract: contract::CONTRACT,
        source_revision: source.into(),
        image: image.into(),
        manifest_digest,
        mode: mode.into(),
        outcome: outcome.into(),
        exit,
        cleanup_verified: cleanup,
        elapsed_ms: started.elapsed().as_millis(),
        worker,
        exclusions: vec![
            "historical-carryover",
            "native-bundles",
            "Windows",
            "macOS",
            "observed-syscall-read-set",
            "user-namespace-isolation",
        ],
    };
    evidence::publish(output, &record)?;
    log.event(
        if outcome == "succeeded" {
            "INFO"
        } else {
            "ERROR"
        },
        "shutdown",
        outcome,
    );
    println!("{}", serde_json::to_string(&record)?);
    if outcome != "succeeded" {
        return Err("Controlled execution did not succeed".into());
    }
    Ok(())
}
