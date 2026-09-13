//! Bounded, read-only Git commands for local source capture, not a runtime launcher.

use crate::WikiError;
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::fd::OwnedFd;
use std::path::Path;
use std::time::Instant;

const GIT_OPTIONS: [&str; 7] = [
    "--no-optional-locks",
    "--no-lazy-fetch",
    "--no-replace-objects",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.hooksPath=/dev/null",
];

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
    run_inner(root, args, limit, deadline, None, None)
}

#[cfg(unix)]
pub(crate) fn run_with_directory_fd(
    directory: &OwnedFd,
    args: &[&str],
    limit: usize,
    deadline: Instant,
    helper: Option<&Path>,
) -> Result<GitOutput, WikiError> {
    run_inner(
        Path::new("/"),
        args,
        limit,
        deadline,
        Some(directory),
        helper,
    )
}

#[cfg(unix)]
fn run_inner(
    root: &Path,
    args: &[&str],
    limit: usize,
    deadline: Instant,
    directory_fd: Option<&OwnedFd>,
    helper: Option<&Path>,
) -> Result<GitOutput, WikiError> {
    use rustix::fs::{fcntl_getfl, fcntl_setfl, OFlags};
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    if Instant::now() >= deadline {
        return Err(failure("source capture deadline reached"));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = helper;
    let mut command;
    #[cfg(target_os = "macos")]
    let helper_spawn = directory_fd.is_some();
    #[cfg(not(target_os = "macos"))]
    let helper_spawn = false;

    if helper_spawn {
        #[cfg(target_os = "macos")]
        {
            // macOS cannot traverse /dev/fd directory entries. The trusted
            // multicall sidecar receives the retained directory as stdin and
            // changes directory by descriptor before replacing itself with Git.
            let helper = helper.ok_or_else(|| failure("bundled source helper unavailable"))?;
            let helper = validate_helper_path(helper)?;
            let directory = directory_fd.ok_or_else(|| failure("source directory unavailable"))?;
            let stdin = rustix::io::dup(directory)
                .map_err(|_| failure("source directory descriptor unavailable"))?;
            command = Command::new(helper);
            command
                .arg0("crew-wiki")
                .arg("__source-git")
                .args(args)
                .stdin(Stdio::from(stdin));
            configure_git_environment(&mut command);
        }
        #[cfg(not(target_os = "macos"))]
        {
            return Err(failure("bundled source helper unavailable"));
        }
    } else {
        command = Command::new("git");
        command.args(GIT_OPTIONS).args(args).stdin(Stdio::null());
        configure_git_environment(&mut command);
        if let Some(directory) = directory_fd {
            #[cfg(target_os = "linux")]
            {
                let directory = format!("/dev/fd/{}", directory.as_raw_fd());
                command
                    .current_dir(&directory)
                    .env("GIT_DIR", format!("{directory}/.git"))
                    .env("GIT_WORK_TREE", &directory);
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = directory;
                return Err(failure(
                    "retained-FD Git source is unavailable on this host",
                ));
            }
        } else {
            command.current_dir(root);
        }
    }
    command
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

fn configure_git_environment(command: &mut std::process::Command) {
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1");
}

#[cfg(target_os = "macos")]
pub(crate) fn bundled_helper_path() -> Result<std::path::PathBuf, WikiError> {
    let executable =
        std::env::current_exe().map_err(|_| failure("bundled source helper unavailable"))?;
    let parent = executable
        .parent()
        .ok_or_else(|| failure("bundled source helper unavailable"))?;
    validate_helper_path(&parent.join("buzz-dev-mcp"))
}

#[cfg(target_os = "macos")]
pub(crate) fn validate_helper_path(path: &Path) -> Result<std::path::PathBuf, WikiError> {
    use std::os::unix::fs::PermissionsExt;
    if !path.is_absolute() {
        return Err(failure("bundled source helper unavailable"));
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| failure("bundled source helper unavailable"))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.permissions().mode() & 0o111 == 0
    {
        return Err(failure("bundled source helper unavailable"));
    }
    Ok(path.to_owned())
}

#[cfg(target_os = "macos")]
pub(crate) fn run_helper(args: Vec<String>) -> i32 {
    use rustix::fs::{fstat, FileType};
    use rustix::io::{fcntl_setfd, FdFlags};
    use rustix::process::fchdir;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    if !valid_helper_args(&args) {
        eprintln!("unsupported source read");
        return 1;
    }
    let stdin = rustix::stdio::stdin();
    let stat = match fstat(stdin) {
        Ok(stat) => stat,
        Err(_) => {
            eprintln!("source directory unavailable");
            return 1;
        }
    };
    if FileType::from_raw_mode(stat.st_mode as _) != FileType::Directory
        || fchdir(stdin).is_err()
        || fcntl_setfd(stdin, FdFlags::CLOEXEC).is_err()
    {
        eprintln!("source directory unavailable");
        return 1;
    }
    let mut command = Command::new("git");
    command
        .args(GIT_OPTIONS)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    configure_git_environment(&mut command);
    command.env("GIT_DIR", ".git").env("GIT_WORK_TREE", ".");
    let _ = command.exec();
    eprintln!("cannot start local Git source read");
    1
}

#[cfg(target_os = "macos")]
fn valid_helper_args(args: &[String]) -> bool {
    match args {
        [command, flag]
            if command == "rev-parse"
                && matches!(flag.as_str(), "--is-inside-work-tree" | "--show-prefix") =>
        {
            true
        }
        [command, kind, oid]
            if command == "cat-file"
                && matches!(kind.as_str(), "commit" | "tree" | "blob")
                && valid_oid(oid) =>
        {
            true
        }
        _ => false,
    }
}

#[cfg(target_os = "macos")]
fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::{run_with_directory_fd, validate_helper_path};
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "crew-source-runner-macos-{}-{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).expect("create unique source runner fixture");
            Self(path)
        }

        fn helper(&self, body: &str) -> PathBuf {
            let path = self.0.join("helper");
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write owned helper");
            let mut permissions = std::fs::metadata(&path)
                .expect("stat owned helper")
                .permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&path, permissions).expect("make owned helper executable");
            path
        }

        fn helper_with_process_probe(&self, body: &str) -> (PathBuf, PathBuf) {
            let path = self.0.join("helper");
            let process_state = PathBuf::from(format!("{}.process-state", path.display()));
            let quoted_process_state = format!(
                "'{}'",
                process_state.to_string_lossy().replace('\'', "'\\''")
            );
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\nprintf '%s %s\\n' \"$$\" \"$(ps -o pgid= -p $$ | tr -d ' ')\" > {quoted_process_state}\n{body}\n"
                ),
            )
            .expect("write owned helper");
            let mut permissions = std::fs::metadata(&path)
                .expect("stat owned helper")
                .permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&path, permissions).expect("make owned helper executable");
            (path, process_state)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).expect("remove only owned source runner fixture");
        }
    }

    #[test]
    fn helper_runner_bounds_timeout_and_process_group_cleanup() {
        let fixture = Fixture::new();
        let (helper, process_state) = fixture.helper_with_process_probe("(sleep 5) &\nsleep 5");
        let directory = std::fs::File::open(&fixture.0).expect("open owned directory");
        let directory = rustix::io::dup(&directory).expect("duplicate owned directory");
        let started = Instant::now();
        let result = run_with_directory_fd(
            &directory,
            &["rev-parse", "--is-inside-work-tree"],
            128,
            started + Duration::from_secs(2),
            Some(&helper),
        );
        assert!(result.is_err(), "a timed out helper must fail");
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "cleanup must stay bounded"
        );
        let fields: Vec<_> = std::fs::read_to_string(process_state)
            .expect("helper must report its process identity")
            .split_whitespace()
            .map(str::parse::<i32>)
            .collect::<Result<_, _>>()
            .expect("helper process identity must be numeric");
        assert_eq!(fields.len(), 2, "helper must report PID and PGID");
        let pid = rustix::process::Pid::from_raw(fields[0]).expect("helper PID must be positive");
        let pgid = rustix::process::Pid::from_raw(fields[1]).expect("helper PGID must be positive");
        assert!(
            rustix::process::test_kill_process(pid).is_err(),
            "timed out helper must be reaped"
        );
        assert!(
            rustix::process::test_kill_process_group(pgid).is_err(),
            "timed out helper process group must be gone"
        );
    }

    #[test]
    fn helper_runner_bounds_output_and_process_group_cleanup() {
        let fixture = Fixture::new();
        let (helper, process_state) =
            fixture.helper_with_process_probe("while :; do printf 0123456789; done");
        let directory = std::fs::File::open(&fixture.0).expect("open owned directory");
        let directory = rustix::io::dup(&directory).expect("duplicate owned directory");
        let result = run_with_directory_fd(
            &directory,
            &["rev-parse", "--is-inside-work-tree"],
            128,
            Instant::now() + Duration::from_secs(5),
            Some(&helper),
        );
        assert!(result.is_err(), "output over the bound must fail");
        let fields: Vec<_> = std::fs::read_to_string(process_state)
            .expect("helper must report its process identity")
            .split_whitespace()
            .map(str::parse::<i32>)
            .collect::<Result<_, _>>()
            .expect("helper process identity must be numeric");
        assert_eq!(fields.len(), 2, "helper must report PID and PGID");
        let pgid = rustix::process::Pid::from_raw(fields[1]).expect("helper PGID must be positive");
        assert!(
            rustix::process::test_kill_process_group(pgid).is_err(),
            "output limit helper process group must be gone"
        );
    }

    #[test]
    fn bundled_helper_rejects_a_symlink() {
        let fixture = Fixture::new();
        let target = fixture.helper(":");
        let link = fixture.0.join("helper-link");
        std::os::unix::fs::symlink(&target, &link).expect("create only owned helper symlink");
        assert!(
            validate_helper_path(&link).is_err(),
            "bundled helper validation must reject symlink targets"
        );
    }
}
