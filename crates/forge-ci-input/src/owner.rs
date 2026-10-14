#[cfg(unix)]
use crate::contract;
use crate::contract::Result;
use std::process::{Command, Stdio};
#[cfg(unix)]
use std::{
    io::Read,
    os::fd::AsRawFd,
    time::{Duration, Instant},
};

pub struct Captured {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr_digest: String,
}
#[cfg(unix)]
static CANCELLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn cancelled() -> bool {
    #[cfg(unix)]
    {
        CANCELLED.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(not(unix))]
    {
        false
    }
}
#[cfg(unix)]
extern "C" fn cancel(_: i32) {
    CANCELLED.store(true, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(unix)]
pub fn initialize() -> Result<()> {
    // Signal handlers cannot allocate or clean up; the owner performs cleanup.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = cancel as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        for signal in [libc::SIGINT, libc::SIGTERM] {
            if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
    }
    Ok(())
}
#[cfg(not(unix))]
pub fn initialize() -> Result<()> {
    Ok(())
}

pub fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    command
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}
#[cfg(unix)]
struct ChildOwner(std::process::Child);
#[cfg(unix)]
impl Drop for ChildOwner {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.0.try_wait().ok().flatten().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
#[cfg(unix)]
pub fn run(mut command: Command, wall: u64, idle: u64) -> Result<Captured> {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
    let cleanup = command
        .get_args()
        .any(|a| a == "rm" || a == "inspect" || a == "ps");
    let mut child = ChildOwner(command.spawn()?);
    let out = child.0.stdout.take().ok_or("Missing stdout")?;
    let err = child.0.stderr.take().ok_or("Missing stderr")?;
    let mut pipes: [Box<dyn Pipe>; 2] = [Box::new(out), Box::new(err)];
    for pipe in &pipes {
        // Nonblocking pipes keep EOF and escaped descendants inside the wall deadline.
        unsafe {
            let flags = libc::fcntl(pipe.as_raw_fd(), libc::F_GETFL);
            if flags < 0
                || libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) < 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
        }
    }
    let start = Instant::now();
    let mut last = start;
    let mut heartbeat = start;
    let mut closed = [false; 2];
    let mut bytes = [Vec::new(), Vec::new()];
    loop {
        if CANCELLED.load(std::sync::atomic::Ordering::Relaxed) && !cleanup {
            return Err("Owned command cancelled".into());
        }
        if start.elapsed().as_secs() >= wall || last.elapsed().as_secs() >= idle {
            return Err("Owned command wall or idle timeout".into());
        }
        if heartbeat.elapsed().as_secs() >= 10 {
            eprintln!(
                "Owned command active elapsed={}s",
                start.elapsed().as_secs()
            );
            heartbeat = Instant::now();
        }
        for index in 0..2 {
            if closed[index] {
                continue;
            }
            let mut buffer = [0; 8192];
            match pipes[index].read(&mut buffer) {
                Ok(0) => closed[index] = true,
                Ok(n) => {
                    if bytes.iter().map(Vec::len).sum::<usize>() + n > 16 * 1024 * 1024 {
                        return Err("Output limit exceeded".into());
                    }
                    bytes[index].extend_from_slice(&buffer[..n]);
                    last = Instant::now();
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e.into()),
            }
        }
        if closed.iter().all(|b| *b)
            && let Some(status) = child.0.try_wait()?
        {
            return Ok(Captured {
                code: status.code().unwrap_or(128),
                stdout: std::mem::take(&mut bytes[0]),
                stderr_digest: contract::digest(&bytes[1]),
            });
        }
        let mut fds: Vec<_> = pipes
            .iter()
            .enumerate()
            .filter(|(i, _)| !closed[*i])
            .map(|(_, p)| libc::pollfd {
                fd: p.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();
        unsafe {
            libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 20);
        }
    }
}
#[cfg(unix)]
trait Pipe: Read + AsRawFd {}
#[cfg(unix)]
impl<T: Read + AsRawFd> Pipe for T {}
#[cfg(not(unix))]
pub fn run(_: Command, _: u64, _: u64) -> Result<Captured> {
    Err("Controlled execution requires Linux".into())
}
