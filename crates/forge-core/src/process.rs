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
    let mut command = Command::new("git");
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
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        );
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|_| {
        Error::new(
            "missing-git",
            "Git is required; install it and retry doctor",
        )
    })?;
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
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(timeout.clamp(1, 300)) {
            #[cfg(windows)]
            {
                let mut kill = Command::new("taskkill");
                use std::os::windows::process::CommandExt;
                kill.creation_flags(0x08000000);
                let _ = kill
                    .args(["/PID", &child.id().to_string(), "/T", "/F"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
            let _ = out.join();
            let _ = err.join();
            return Err(Error::new("timeout", "Git operation exceeded deadline"));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
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
