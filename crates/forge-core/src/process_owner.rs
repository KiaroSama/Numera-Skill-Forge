#[cfg(windows)]
mod windows {
    use crate::{Error, Result};
    use std::{ffi::c_void, os::windows::io::AsRawHandle, process::Child};
    type Handle = *mut c_void;
    #[repr(C)]
    struct Basic {
        process_time: i64,
        job_time: i64,
        flags: u32,
        minimum: usize,
        maximum: usize,
        active: u32,
        affinity: usize,
        priority: u32,
        scheduling: u32,
    }
    #[repr(C)]
    struct Io {
        read: u64,
        write: u64,
        other: u64,
        read_bytes: u64,
        write_bytes: u64,
        other_bytes: u64,
    }
    #[repr(C)]
    struct Extended {
        basic: Basic,
        io: Io,
        process_memory: usize,
        job_memory: usize,
        peak_process: usize,
        peak_job: usize,
    }
    #[repr(C)]
    struct ThreadEntry {
        size: u32,
        usage: u32,
        id: u32,
        owner: u32,
        base: i32,
        delta: i32,
        flags: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(
            job: Handle,
            class: i32,
            information: *const c_void,
            length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn TerminateJobObject(job: Handle, code: u32) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn CreateToolhelp32Snapshot(flags: u32, process: u32) -> Handle;
        fn Thread32First(snapshot: Handle, entry: *mut ThreadEntry) -> i32;
        fn Thread32Next(snapshot: Handle, entry: *mut ThreadEntry) -> i32;
        fn OpenThread(access: u32, inherit: i32, id: u32) -> Handle;
        fn ResumeThread(thread: Handle) -> u32;
    }
    pub struct Owner(Handle);
    impl Owner {
        pub fn attach(child: &mut Child) -> Result<Self> {
            // The zeroed C structure sets only documented KILL_ON_JOB_CLOSE; no borrowed pointers.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error::new(
                        "process-owner",
                        "Cannot create subprocess ownership job",
                    ));
                }
                let mut limits: Extended = std::mem::zeroed();
                limits.basic.flags = 0x2000;
                if SetInformationJobObject(
                    job,
                    9,
                    &limits as *const _ as *const c_void,
                    std::mem::size_of::<Extended>() as u32,
                ) == 0
                    || AssignProcessToJobObject(job, child.as_raw_handle()) == 0
                {
                    let _ = child.kill();
                    let _ = child.wait();
                    CloseHandle(job);
                    return Err(Error::new(
                        "process-owner",
                        "Cannot enforce subprocess tree ownership",
                    ));
                }
                // Spawned suspended: its only thread cannot create descendants before job assignment.
                let snapshot = CreateToolhelp32Snapshot(4, 0);
                let mut resumed = false;
                if snapshot as isize != -1 {
                    let mut entry: ThreadEntry = std::mem::zeroed();
                    entry.size = std::mem::size_of::<ThreadEntry>() as u32;
                    let mut found = Thread32First(snapshot, &mut entry);
                    while found != 0 {
                        if entry.owner == child.id() {
                            let thread = OpenThread(2, 0, entry.id);
                            if !thread.is_null() {
                                resumed = ResumeThread(thread) != u32::MAX;
                                CloseHandle(thread);
                            }
                            break;
                        }
                        found = Thread32Next(snapshot, &mut entry);
                    }
                    CloseHandle(snapshot);
                }
                if !resumed {
                    TerminateJobObject(job, 124);
                    let _ = child.wait();
                    CloseHandle(job);
                    return Err(Error::new(
                        "process-owner",
                        "Cannot resume owned suspended subprocess",
                    ));
                }
                Ok(Self(job))
            }
        }
        pub fn terminate(&self) {
            unsafe {
                TerminateJobObject(self.0, 124);
            }
        }
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}
#[cfg(windows)]
pub use windows::Owner;
#[cfg(unix)]
pub struct Owner(u32);
#[cfg(unix)]
impl Owner {
    pub fn attach(child: &mut std::process::Child) -> crate::Result<Self> {
        Ok(Self(child.id()))
    }
    pub fn terminate(&self) {
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", self.0)])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}
#[cfg(unix)]
impl Drop for Owner {
    fn drop(&mut self) {
        self.terminate();
    }
}
