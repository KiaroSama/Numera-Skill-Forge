use crate::{Error, Result};
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn git(args: &[String], timeout: u64) -> Result<Vec<u8>> {
    git_env(args, timeout, &[])
}
pub fn git_env(args: &[String], timeout: u64, environment: &[(&str, &str)]) -> Result<Vec<u8>> {
    git_input(args, timeout, environment, None)
}
pub fn git_input(
    args: &[String],
    timeout: u64,
    environment: &[(&str, &str)],
    input: Option<Vec<u8>>,
) -> Result<Vec<u8>> {
    git_command(args, timeout, environment, input, false)
}
pub fn git_isolated(
    args: &[String],
    timeout: u64,
    environment: &[(&str, &str)],
) -> Result<Vec<u8>> {
    git_command(args, timeout, environment, None, true)
}
fn git_command(
    args: &[String],
    timeout: u64,
    environment: &[(&str, &str)],
    input: Option<Vec<u8>>,
    isolated: bool,
) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    if isolated {
        command.env_clear();
        for key in [
            "PATH",
            "PATHEXT",
            "SYSTEMROOT",
            "WINDIR",
            "COMSPEC",
            "TEMP",
            "TMP",
            "TMPDIR",
            "LANG",
            "LC_ALL",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
    }
    for (key, value) in environment {
        command.env(key, value);
    }
    command.args([
        "-c",
        "core.hooksPath=",
        "-c",
        "protocol.file.allow=never",
        "-c",
        "protocol.ext.allow=never",
        "-c",
        "http.followRedirects=false",
        "-c",
        "filter.lfs.required=false",
    ]);
    command
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        );
    execute(command, timeout, input)
}

pub fn github(args: &[String], timeout: u64) -> Result<Vec<u8>> {
    let mut command = Command::new("gh");
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    execute(command, timeout, None)
}
fn execute(mut command: Command, timeout: u64, input: Option<Vec<u8>>) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000004);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    crate::jobs::checkpoint()?;
    let mut child = command.spawn().map_err(|_| {
        Error::new(
            "missing-prerequisite",
            "Required Git or GitHub CLI executable is unavailable; inspect doctor",
        )
    })?;
    let owner = crate::process_owner::Owner::attach(&mut child)?;
    let input_writer = if let Some(bytes) = input {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::new("process", "Missing input pipe"))?;
        Some(std::thread::spawn(move || {
            use std::io::Write;
            stdin.write_all(&bytes)
        }))
    } else {
        None
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::new("process", "Missing stdout pipe"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::new("process", "Missing stderr pipe"))?;
    let out = std::thread::spawn(move || capture(stdout));
    let err = std::thread::spawn(move || capture(stderr));
    let start = Instant::now();
    let mut exit_status = None;
    let status = loop {
        if exit_status.is_none() {
            exit_status = child.try_wait()?;
        }
        if let Some(status) = exit_status
            && out.is_finished()
            && err.is_finished()
            && input_writer
                .as_ref()
                .is_none_or(|writer| writer.is_finished())
        {
            break status;
        }
        let interrupted = crate::jobs::checkpoint().err();
        if interrupted.is_some() || start.elapsed() > Duration::from_secs(timeout.clamp(1, 300)) {
            owner.terminate();
            let _ = child.kill();
            let _ = child.wait();
            if let Some(writer) = input_writer {
                let _ = writer.join();
            }
            let _ = out.join();
            let _ = err.join();
            return Err(interrupted
                .unwrap_or_else(|| Error::new("timeout", "Git operation exceeded deadline")));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if let Some(writer) = input_writer {
        writer
            .join()
            .map_err(|_| Error::new("process", "Input writer failed"))??;
    }
    let output = out
        .join()
        .map_err(|_| Error::new("process", "Output reader failed"))??;
    let _error = err
        .join()
        .map_err(|_| Error::new("process", "Error reader failed"))??;
    if !status.success() {
        return Err(Error::new(
            "git-failed",
            format!(
                "Git exited with {}; credentials, URL or ref may require attention",
                status.code().unwrap_or(-1)
            ),
        ));
    }
    Ok(output)
}
fn capture(reader: impl Read) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(crate::storage::MAX_TOTAL + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > crate::storage::MAX_TOTAL {
        return Err(Error::new(
            "output-limit",
            "Process output exceeded 256 MiB",
        ));
    }
    Ok(bytes)
}
pub fn text(args: &[&str]) -> Result<String> {
    String::from_utf8(git(
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        60,
    )?)
    .map_err(|_| Error::new("encoding", "Git output is not UTF-8"))
}
