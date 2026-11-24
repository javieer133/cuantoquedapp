//! Child processes whose whole tree dies with them: a hung `claude` or `codex` may have
//! started children of its own, and killing only the direct child would leave them running.
//!
//! - Unix: the child leads a new process group, and the whole group gets `SIGKILL`. While
//!   a group has members its number is never reused, so this cannot hit unrelated processes.
//! - Windows: the child goes into a Job Object that kills every process in it when it is
//!   closed (also if this app dies). Children it starts join the job on their own; only one
//!   started in the instant between spawning and joining the job could escape.
use std::{
    io,
    process::{Child, Command},
};

/// A child process that, when dropped, is killed together with everything it started
/// (also what it left running after exiting on its own) and reaped.
pub struct ProcessTree {
    pub child: Child,
    #[cfg(windows)]
    job: Option<job::Job>,
}

impl ProcessTree {
    /// Spawns `command` in its own tree. On Windows it also hides the console window (a
    /// GUI app has none: without the flag one flashes on every start).
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let child = command.spawn()?;
        Ok(Self {
            #[cfg(windows)]
            job: job::Job::holding(&child),
            child,
        })
    }

    #[cfg(unix)]
    fn kill_rest(&mut self) {
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        const SIGKILL: i32 = 9;
        // `spawn` made the child its group leader: group number = its PID. Never 0 or 1:
        // `kill(-1)` would signal every process of the user. If the child already exited
        // with nothing left, the group is gone and the call just fails.
        if let Ok(group) = i32::try_from(self.child.id())
            && group > 1
        {
            // SAFETY: `kill` only takes integers; a group that no longer exists is an error.
            unsafe {
                kill(-group, SIGKILL);
            }
        }
    }

    #[cfg(windows)]
    fn kill_rest(&mut self) {
        if let Some(job) = &self.job {
            job.terminate();
        }
    }

    #[cfg(not(any(unix, windows)))]
    fn kill_rest(&mut self) {}
}

impl Drop for ProcessTree {
    /// The rest of the tree goes first, while the child is not reaped yet (its number
    /// stays in use).
    fn drop(&mut self) {
        self.kill_rest();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(windows)]
mod job {
    use std::{os::windows::io::AsRawHandle, process::Child};

    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject,
        },
    };

    /// An anonymous job that kills its processes when its last handle closes.
    pub struct Job(HANDLE);

    // SAFETY: a job handle is a kernel object handle, usable from any thread.
    unsafe impl Send for Job {}

    impl Job {
        /// A new job holding `child`, or `None` if Windows refuses (then only the child
        /// itself is killed, as before).
        pub fn holding(child: &Child) -> Option<Self> {
            // SAFETY: plain Win32 calls with valid arguments; the handle is owned by `Job`
            // from the moment it exists, so every failure path closes it.
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return None;
                }
                let job = Self(handle);
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let set = SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    (&raw const limits).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if set == 0 || AssignProcessToJobObject(handle, child.as_raw_handle()) == 0 {
                    return None;
                }
                Some(job)
            }
        }

        pub fn terminate(&self) {
            // SAFETY: `self.0` is a valid job handle until `drop`.
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: closed exactly once; closing it also kills what is left in the job.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(all(test, unix))]
pub(crate) mod test_support {
    use std::{
        path::Path,
        process::{Command, Stdio},
        time::Duration,
    };

    fn pid_is_alive(pidfile: &Path) -> bool {
        let pid = std::fs::read_to_string(pidfile)
            .map(|text| text.trim().to_string())
            .unwrap_or_default();
        !pid.is_empty()
            && Command::new("kill")
                .args(["-0", &pid])
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
    }

    /// True once the PID written in `pidfile` no longer exists (waits up to 3 s: a killed
    /// process stays a zombie until its new parent reaps it).
    pub fn process_is_gone(pidfile: &Path) -> bool {
        for _ in 0..30 {
            if !pid_is_alive(pidfile) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }
}
