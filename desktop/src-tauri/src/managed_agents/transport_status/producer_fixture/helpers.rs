//! Bounded support for the ignored whole-chain producer fixture.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, ExitStatus};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command as TokioCommand};
use tokio::task::JoinHandle;

use buzz_core_pkg::transport_status::MAX_RECORD_BYTES;

use super::{
    ACP_BINARY_ENV, ACP_BINARY_SHA256_ENV, ACP_SOURCE_ROOT_ENV, ACP_SOURCE_SHA256_ENV,
    CAPTURED_BINARY_BYTES, CAPTURED_SOURCE_BYTES, CAPTURED_SOURCE_FILES, CAPTURED_STREAM_BYTES,
    CAPTURE_DRAIN_DEADLINE, POLL_INTERVAL,
};

pub(super) fn verify_supplied_binary_and_source(deadline: Instant) -> Result<PathBuf, String> {
    check_deadline(deadline, "binary/source preflight")?;
    let binary = required_absolute_path(ACP_BINARY_ENV)?;
    let expected_binary_sha = required_digest(ACP_BINARY_SHA256_ENV)?;
    let actual_binary_sha = sha256_file(&binary, deadline)?;
    if actual_binary_sha != expected_binary_sha {
        return Err(format!(
            "{ACP_BINARY_ENV} SHA256 mismatch: expected {expected_binary_sha}, got {actual_binary_sha}"
        ));
    }
    let source_root = required_absolute_directory_path(ACP_SOURCE_ROOT_ENV)?;
    let expected_source_sha = required_digest(ACP_SOURCE_SHA256_ENV)?;
    let expected_checkout_source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/buzz-acp")
        .canonicalize()
        .map_err(|error| format!("resolve checked-out ACP source root: {error}"))?;
    if source_root != expected_checkout_source {
        return Err(format!(
            "{ACP_SOURCE_ROOT_ENV} must bind the checked-out ACP source root {}",
            expected_checkout_source.display()
        ));
    }
    let actual_source_sha = source_tree_sha256(&source_root, deadline)?;
    if actual_source_sha != expected_source_sha {
        return Err(format!(
            "{ACP_SOURCE_ROOT_ENV} SHA256 mismatch: expected {expected_source_sha}, got {actual_source_sha}"
        ));
    }
    Ok(binary)
}

fn required_absolute_path(name: &str) -> Result<PathBuf, String> {
    let value = std::env::var_os(name).ok_or_else(|| format!("{name} must be supplied"))?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(format!("{name} must be an absolute path"));
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("canonicalize {name}: {error}"))?;
    let metadata = fs::metadata(&canonical).map_err(|error| format!("stat {name}: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("{name} must name a regular file"));
    }
    Ok(canonical)
}

fn required_absolute_directory_path(name: &str) -> Result<PathBuf, String> {
    let value = std::env::var_os(name).ok_or_else(|| format!("{name} must be supplied"))?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(format!("{name} must be an absolute path"));
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("canonicalize {name}: {error}"))?;
    let metadata = fs::metadata(&canonical).map_err(|error| format!("stat {name}: {error}"))?;
    if !metadata.is_dir() {
        return Err(format!("{name} must name a directory"));
    }
    Ok(canonical)
}

fn required_digest(name: &str) -> Result<String, String> {
    let digest = std::env::var(name).map_err(|_| format!("{name} must be supplied"))?;
    if !is_lower_hex_64(&digest) {
        return Err(format!("{name} must be a lower-case SHA256 digest"));
    }
    Ok(digest)
}

fn sha256_file(path: &Path, deadline: Instant) -> Result<String, String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("stat {}: {error}", path.display()))?;
    if metadata.len() > CAPTURED_BINARY_BYTES {
        return Err(format!(
            "supplied ACP binary exceeds its {} byte bound",
            CAPTURED_BINARY_BYTES
        ));
    }
    let mut file = File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        check_deadline(deadline, "binary/source hash")?;
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("hash {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub(super) fn read_bounded_file(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| format!("read {}: {error}", path.display()))?
        .take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("final status file exceeded the production byte cap".into());
    }
    Ok(bytes)
}

/// Hash every regular file in the supplied ACP checkout in lexical path order.
/// The relative pathname is included before its bytes so two source trees with
/// the same concatenated contents cannot collide as a binding manifest.
fn source_tree_sha256(root: &Path, deadline: Instant) -> Result<String, String> {
    let mut files = Vec::new();
    collect_source_files(root, root, &mut files, &mut 0, &mut 0, deadline)?;
    files.sort();
    let mut hasher = Sha256::new();
    for relative in files {
        check_deadline(deadline, "source tree hash")?;
        let path = root.join(&relative);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("stat source {}: {error}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!(
                "source binding entry is no longer a regular file: {}",
                path.display()
            ));
        }
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        let mut file = File::open(&path)
            .map_err(|error| format!("open source {}: {error}", path.display()))?;
        let mut bytes = [0_u8; 64 * 1024];
        let mut total = 0_u64;
        loop {
            check_deadline(deadline, "source tree hash")?;
            let read = file
                .read(&mut bytes)
                .map_err(|error| format!("read source {}: {error}", path.display()))?;
            if read == 0 {
                break;
            }
            total = total.saturating_add(read as u64);
            if total > CAPTURED_SOURCE_BYTES {
                return Err(format!(
                    "source binding file exceeds {} bytes: {}",
                    CAPTURED_SOURCE_BYTES,
                    path.display()
                ));
            }
            hasher.update(&bytes[..read]);
        }
        hasher.update([0]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn collect_source_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
    file_count: &mut usize,
    byte_count: &mut u64,
    deadline: Instant,
) -> Result<(), String> {
    check_deadline(deadline, "source tree walk")?;
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("read source directory {}: {error}", directory.display()))?;
    for entry in entries {
        check_deadline(deadline, "source tree walk")?;
        let entry = entry.map_err(|error| format!("read source entry: {error}"))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("stat source entry {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "source binding contains a symlink: {}",
                path.display()
            ));
        }
        if metadata.is_dir() {
            collect_source_files(root, &path, files, file_count, byte_count, deadline)?;
        } else if metadata.is_file() {
            *file_count = file_count.saturating_add(1);
            if *file_count > CAPTURED_SOURCE_FILES {
                return Err("source binding contains too many files".into());
            }
            *byte_count = byte_count.saturating_add(metadata.size());
            if *byte_count > CAPTURED_SOURCE_BYTES {
                return Err("source binding exceeds its byte bound".into());
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "source binding path escaped its root".to_string())?
                .to_path_buf();
            files.push(relative);
        }
    }
    Ok(())
}

fn check_deadline(deadline: Instant, phase: &str) -> Result<(), String> {
    if Instant::now() >= deadline {
        Err(format!("{phase} exceeded the fixture deadline"))
    } else {
        Ok(())
    }
}

pub(super) fn assert_no_provider_or_user_secrets_in_child_env(command: &StdCommand) {
    for name in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "BUZZ_PRIVATE_KEY",
        "BUZZ_AUTH_TAG",
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "HERMES_HOME",
    ] {
        assert!(
            command.get_envs().all(|(key, _)| key != OsStr::new(name)),
            "fixture child must not inherit {name}"
        );
    }
}

pub(super) fn write_poison_provider(path: &Path, marker: &Path) -> Result<(), String> {
    let script = format!(
        "#!/bin/sh\nprintf '%s' poison-provider-invoked > '{}'\nexit 97\n",
        marker.display()
    );
    write_private_file(path, script.as_bytes())?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("chmod poison provider: {error}"))
}

pub(super) fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("private file has no parent: {}", path.display()))?;
    create_private_dir(parent)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create private file {}: {error}", path.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("write private file {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("sync private file {}: {error}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("chmod private file {}: {error}", path.display()))
}

pub(super) fn create_private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|error| format!("create private directory {}: {error}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("chmod private directory {}: {error}", path.display()))
}

pub(super) fn now_ms() -> Result<u64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "fixture clock is before the Unix epoch".to_string())?
        .as_millis();
    u64::try_from(millis).map_err(|_| "fixture clock overflowed u64 milliseconds".into())
}

pub(super) fn remaining_until(deadline: Instant) -> Result<Duration, String> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| "fixture deadline elapsed".to_string())
}

pub(super) fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) struct FixtureRoot {
    pub(super) _root: TempDir,
    pub(super) bin: PathBuf,
    pub(super) config: PathBuf,
    pub(super) data: PathBuf,
    pub(super) cache: PathBuf,
    pub(super) home: PathBuf,
    pub(super) cwd: PathBuf,
    pub(super) logs: PathBuf,
    pub(super) tmp: PathBuf,
}

impl FixtureRoot {
    pub(super) fn new() -> Result<Self, String> {
        let root = tempfile::Builder::new()
            .prefix("crew-338-producer-")
            .tempdir()
            .map_err(|error| format!("create fixture tempdir: {error}"))?;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("chmod fixture tempdir: {error}"))?;
        let paths = [
            "bin", "config", "data", "cache", "home", "cwd", "logs", "tmp",
        ]
        .map(|name| root.path().join(name));
        for path in &paths {
            create_private_dir(path)?;
        }
        let [bin, config, data, cache, home, cwd, logs, tmp] = paths;
        Ok(Self {
            _root: root,
            bin,
            config,
            data,
            cache,
            home,
            cwd,
            logs,
            tmp,
        })
    }
}

pub(super) struct CapturedOutput {
    pub(super) bytes: Vec<u8>,
    pub(super) truncated: bool,
    error: Option<String>,
}

fn capture_stream<R>(mut reader: R, deadline: Instant) -> JoinHandle<CapturedOutput>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut bytes = Vec::with_capacity(CAPTURED_STREAM_BYTES.min(4096));
        let mut buffer = [0_u8; 8192];
        let mut truncated = false;
        let mut error = None;
        let capture_deadline = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline));
        tokio::pin!(capture_deadline);
        loop {
            tokio::select! {
                _ = &mut capture_deadline => {
                    truncated = true;
                    break;
                }
                result = reader.read(&mut buffer) => {
                    match result {
                        Ok(0) => break,
                        Ok(read) => {
                            let remaining = CAPTURED_STREAM_BYTES.saturating_sub(bytes.len());
                            if read > remaining {
                                truncated = true;
                            }
                            if remaining > 0 {
                                bytes.extend_from_slice(&buffer[..read.min(remaining)]);
                            }
                        }
                        Err(read_error) => {
                            error = Some(read_error.to_string());
                            break;
                        }
                    }
                }
            }
        }
        CapturedOutput {
            bytes,
            truncated,
            error,
        }
    })
}

pub(super) struct ChildOutput {
    pub(super) stdout: CapturedOutput,
    pub(super) stderr: CapturedOutput,
}

pub(super) struct ChildGuard {
    pid: u32,
    child: Option<Child>,
    stdout: Option<JoinHandle<CapturedOutput>>,
    stderr: Option<JoinHandle<CapturedOutput>>,
}

impl ChildGuard {
    pub(super) fn spawn(command: StdCommand, deadline: Instant) -> Result<Self, String> {
        let mut child = TokioCommand::from(command)
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| format!("spawn supplied ACP binary: {error}"))?;
        let child_pid = match child.id() {
            Some(pid) => pid,
            None => {
                let _ = child.start_kill();
                return Err("ACP fixture child did not expose a process ID".into());
            }
        };
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                terminate_process_group(child_pid);
                let _ = child.start_kill();
                return Err("ACP fixture stdout was not piped".into());
            }
        };
        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                terminate_process_group(child_pid);
                let _ = child.start_kill();
                return Err("ACP fixture stderr was not piped".into());
            }
        };
        Ok(Self {
            pid: child_pid,
            child: Some(child),
            stdout: Some(capture_stream(stdout, deadline)),
            stderr: Some(capture_stream(stderr, deadline)),
        })
    }

    pub(super) fn pid(&self) -> Result<u32, String> {
        Ok(self.pid)
    }

    pub(super) async fn wait_until_exit(
        &mut self,
        deadline: Instant,
    ) -> Result<ExitStatus, String> {
        loop {
            let child = self
                .child
                .as_mut()
                .ok_or_else(|| "ACP fixture child was already consumed".to_string())?;
            if let Some(status) = child
                .try_wait()
                .map_err(|error| format!("poll ACP child: {error}"))?
            {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err("ACP fixture child did not exit before its deadline".into());
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    pub(super) async fn finish(&mut self, status: ExitStatus) -> Result<ChildOutput, String> {
        let child_pid = self.pid()?;
        // The child has been reaped, but a faulty fixture or provider could
        // still hold one of its pipes open.  Kill the generation's process
        // group before awaiting the bounded readers so no descendant can keep
        // this test alive.
        terminate_process_group(child_pid);
        self.child.take();
        let stdout = finish_capture(&mut self.stdout, "stdout").await?;
        let stderr = finish_capture(&mut self.stderr, "stderr").await?;
        if status.success() {
            return Err("ACP fixture unexpectedly exited successfully after AUTH denial".into());
        }
        Ok(ChildOutput { stdout, stderr })
    }
}

async fn finish_capture(
    handle: &mut Option<JoinHandle<CapturedOutput>>,
    stream: &str,
) -> Result<CapturedOutput, String> {
    let mut task = handle
        .take()
        .ok_or_else(|| format!("ACP fixture {stream} collector missing"))?;
    let timeout = CAPTURE_DRAIN_DEADLINE + Duration::from_millis(100);
    match tokio::time::timeout(timeout, &mut task).await {
        Ok(Ok(output)) => {
            if let Some(error) = output.error.as_deref() {
                return Err(format!("ACP fixture {stream} capture failed: {error}"));
            }
            Ok(output)
        }
        Ok(Err(error)) => Err(format!("ACP fixture {stream} collector failed: {error}")),
        Err(_) => {
            task.abort();
            let _ = task.await;
            Err(format!(
                "ACP fixture {stream} capture exceeded its deadline"
            ))
        }
    }
}

fn terminate_process_group(pid: u32) {
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    if pid <= 0 {
        return;
    }
    let _ = nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(-pid),
        nix::sys::signal::Signal::SIGKILL,
    );
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            terminate_process_group(self.pid);
            let _ = child.start_kill();
        }
        if let Some(stdout) = self.stdout.take() {
            stdout.abort();
        }
        if let Some(stderr) = self.stderr.take() {
            stderr.abort();
        }
    }
}
