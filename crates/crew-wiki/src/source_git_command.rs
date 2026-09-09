//! Bounded, read-only Git commands for local source capture, not a runtime launcher.

use crate::WikiError;
use std::path::Path;
use std::time::Instant;

pub(crate) struct GitOutput {
    pub(crate) success: bool,
    pub(crate) bytes: Vec<u8>,
}

#[cfg(not(unix))]
pub(crate) fn run(
    _root: &Path,
    _args: &[&str],
    _limit: usize,
    _deadline: Instant,
) -> Result<GitOutput, WikiError> {
    Err(WikiError::Git(
        "bounded source capture is unavailable on this host".into(),
    ))
}

#[cfg(unix)]
pub(crate) fn run(
    root: &Path,
    args: &[&str],
    limit: usize,
    deadline: Instant,
) -> Result<GitOutput, WikiError> {
    use rustix::fs::{fcntl_getfl, fcntl_setfl, OFlags};
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    if Instant::now() >= deadline {
        return Err(failure("source capture deadline reached"));
    }
    let mut command = Command::new("git");
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .args([
            "--no-optional-locks",
            "--no-lazy-fetch",
            "--no-replace-objects",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let child = command
        .spawn()
        .map_err(|_| failure("cannot start local Git source read"))?;
    let mut owned = OwnedGit {
        child,
        reaped: false,
    };
    let result = (|| {
        let mut stdout = owned
            .child
            .stdout
            .take()
            .ok_or_else(|| failure("Git stdout unavailable"))?;
        let mut stderr = owned
            .child
            .stderr
            .take()
            .ok_or_else(|| failure("Git stderr unavailable"))?;
        fcntl_setfl(
            &stdout,
            fcntl_getfl(&stdout).map_err(|_| failure("Git pipe unavailable"))? | OFlags::NONBLOCK,
        )
        .map_err(|_| failure("Git pipe unavailable"))?;
        fcntl_setfl(
            &stderr,
            fcntl_getfl(&stderr).map_err(|_| failure("Git pipe unavailable"))? | OFlags::NONBLOCK,
        )
        .map_err(|_| failure("Git pipe unavailable"))?;
        let command_deadline = deadline.min(Instant::now() + Duration::from_secs(15));
        let mut bytes = Vec::new();
        let mut errors = Vec::new();
        let mut stdout_done = false;
        let mut stderr_done = false;
        loop {
            stdout_done |= drain(&mut stdout, &mut bytes, limit)?;
            stderr_done |= drain(&mut stderr, &mut errors, 64 * 1024)?;
            // Reap only after both pipes close. This retains the child PID while
            // a containment cleanup might need to address its process group.
            if stdout_done && stderr_done {
                if let Some(status) = owned
                    .child
                    .try_wait()
                    .map_err(|_| failure("Git wait failed"))?
                {
                    owned.reaped = true;
                    return Ok(GitOutput {
                        success: status.success(),
                        bytes,
                    });
                }
            }
            if Instant::now() >= command_deadline {
                return Err(failure("source capture deadline reached"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    })();
    if !owned.reaped {
        owned.stop()?;
    }
    result
}

#[cfg(unix)]
fn failure(message: &str) -> WikiError {
    WikiError::Git(message.into())
}

#[cfg(unix)]
fn drain(
    reader: &mut impl std::io::Read,
    output: &mut Vec<u8>,
    limit: usize,
) -> Result<bool, WikiError> {
    let mut buffer = [0_u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(true),
            Ok(count) => {
                if output.len().saturating_add(count) > limit {
                    return Err(failure("Git source output limit exceeded"));
                }
                output.extend_from_slice(&buffer[..count]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(failure("Git source pipe read failed")),
        }
    }
}

#[cfg(unix)]
struct OwnedGit {
    child: std::process::Child,
    reaped: bool,
}

#[cfg(unix)]
impl OwnedGit {
    fn stop(&mut self) -> Result<(), WikiError> {
        use rustix::process::{kill_process_group, Pid, Signal};
        use std::time::Duration;
        if self.reaped {
            return Ok(());
        }
        let pid = i32::try_from(self.child.id())
            .ok()
            .and_then(Pid::from_raw)
            .ok_or_else(|| failure("Git process identity unavailable"))?;
        // Only this controlled Git child's group; never an arbitrary PID from a caller.
        if let Err(error) = kill_process_group(pid, Signal::KILL) {
            if error != rustix::io::Errno::SRCH {
                let _ = self.child.kill();
                return Err(failure("Git process-group cleanup failed"));
            }
        }
        let _ = self.child.kill();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if self
                .child
                .try_wait()
                .map_err(|_| failure("Git cleanup wait failed"))?
                .is_some()
            {
                self.reaped = true;
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(failure("Git source cleanup did not finish"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

#[cfg(unix)]
impl Drop for OwnedGit {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.stop();
        }
    }
}
