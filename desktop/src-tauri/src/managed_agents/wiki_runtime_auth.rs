//! Disposable authentication handoff for the installed Wiki runtimes.
//!
//! The desktop process reads the existing subscription credential only long
//! enough to prepare a child process. Claude receives an access token through
//! its child environment. Codex requires a small `auth.json` in its isolated
//! `CODEX_HOME`; that file contains no API key and no refresh token. Hermes
//! owns authentication in its staged profile and needs no handoff here.

#[cfg(any(test, all(feature = "system-keyring", target_os = "macos")))]
use super::discovery::bounded_command::{
    output_with_policy, BoundedFailure, BoundedPolicy, OutputBudget,
};
use super::wiki_runtime::WikiRuntimeFailure;
use base64::Engine;
use serde_json::{Map, Value};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
#[cfg(any(test, all(feature = "system-keyring", target_os = "macos")))]
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

const CODEX_AUTH_FILE_LIMIT: u64 = 64 * 1024;
#[cfg(any(test, all(feature = "system-keyring", target_os = "macos")))]
const CLAUDE_CREDENTIALS_LIMIT: usize = 64 * 1024;
const CREDENTIAL_VALUE_LIMIT: usize = 32 * 1024;
#[cfg(all(feature = "system-keyring", target_os = "macos"))]
const CLAUDE_AUTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
#[cfg(all(feature = "system-keyring", target_os = "macos"))]
const CLAUDE_AUTH_STDERR_LIMIT: u64 = 16 * 1024;
#[cfg(all(feature = "system-keyring", target_os = "macos"))]
const CLAUDE_KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// Stage existing subscription authentication for one installed runtime.
///
/// The returned value is an in-memory Claude OAuth access token. Codex is
/// staged into `state_dir/config/auth.json` because the CLI reads that file;
/// callers must set `CODEX_HOME` to that directory. Hermes returns no value
/// because its selected profile owns its authentication state.
pub(crate) fn stage_runtime_auth(
    runtime_id: &str,
    state_dir: &Path,
    cancelled: &AtomicBool,
) -> Result<Option<String>, WikiRuntimeFailure> {
    match runtime_id {
        "hermes" => Ok(None),
        "claude" => read_claude_access_token(cancelled).map(Some),
        "codex" => {
            let source = codex_auth_path()?;
            let source_bytes = read_bounded_file(&source, CODEX_AUTH_FILE_LIMIT)?;
            stage_codex_auth_bytes(&source_bytes, &state_dir.join("config").join("auth.json"))?;
            Ok(None)
        }
        other => Err(WikiRuntimeFailure::UnsupportedRuntime(other.to_owned())),
    }
}

fn codex_auth_path() -> Result<PathBuf, WikiRuntimeFailure> {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|path| path.join(".codex")))
        .ok_or(WikiRuntimeFailure::AuthenticationUnavailable)?;
    if !home.is_absolute() {
        return Err(WikiRuntimeFailure::AuthenticationUnavailable);
    }
    Ok(home.join("auth.json"))
}

fn read_bounded_file(path: &Path, limit: u64) -> Result<Vec<u8>, WikiRuntimeFailure> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Open nonblocking before fstat so a hostile FIFO cannot hold the
        // worker while the auth file is inspected. O_NOFOLLOW rejects a
        // symlink atomically; regular-file reads need no flag clearing.
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|_| WikiRuntimeFailure::AuthenticationUnavailable)?;
    let metadata = file
        .metadata()
        .map_err(|_| WikiRuntimeFailure::AuthenticationUnavailable)?;
    if !metadata.is_file() {
        return Err(WikiRuntimeFailure::AuthenticationUnavailable);
    }
    if metadata.len() > limit {
        return Err(WikiRuntimeFailure::AuthenticationLimit);
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| WikiRuntimeFailure::AuthenticationUnavailable)?;
    if bytes.len() as u64 > limit {
        return Err(WikiRuntimeFailure::AuthenticationLimit);
    }
    Ok(bytes)
}

/// Parse and stage the supported subset of an existing Codex auth file.
///
/// This is kept as the production writer seam so tests can prove that API
/// keys and refresh tokens are omitted from the child file. `source` is never
/// written back or modified.
pub(crate) fn stage_codex_auth_bytes(
    source: &[u8],
    destination: &Path,
) -> Result<(), WikiRuntimeFailure> {
    if source.len() > CODEX_AUTH_FILE_LIMIT as usize {
        return Err(WikiRuntimeFailure::AuthenticationLimit);
    }
    let source: Value =
        serde_json::from_slice(source).map_err(|_| WikiRuntimeFailure::InvalidAuthentication)?;
    let staged = parse_codex_auth(&source)?;
    write_private_json(destination, staged)
}

fn parse_codex_auth(source: &Value) -> Result<Value, WikiRuntimeFailure> {
    let root = source
        .as_object()
        .ok_or(WikiRuntimeFailure::InvalidAuthentication)?;
    let tokens = root
        .get("tokens")
        .and_then(Value::as_object)
        .ok_or(WikiRuntimeFailure::InvalidAuthentication)?;
    let access_token = required_string(tokens, "access_token")?;
    let id_token = required_string(tokens, "id_token")?;
    let account_id = required_string(tokens, "account_id")?;
    let last_refresh = root
        .get("last_refresh")
        .filter(|value| value.as_str().is_some_and(|value| !value.is_empty()))
        .cloned()
        .ok_or(WikiRuntimeFailure::InvalidAuthentication)?;
    validate_access_token_expiry(access_token)?;

    let mut staged_tokens = Map::new();
    staged_tokens.insert("access_token".into(), Value::String(access_token.into()));
    staged_tokens.insert("id_token".into(), Value::String(id_token.into()));
    staged_tokens.insert("account_id".into(), Value::String(account_id.into()));
    // Codex 0.154 requires this key even when the isolated process has no
    // refresh credential. An empty value prevents refresh-token forwarding.
    staged_tokens.insert("refresh_token".into(), Value::String(String::new()));

    let mut staged = Map::new();
    staged.insert("tokens".into(), Value::Object(staged_tokens));
    staged.insert("last_refresh".into(), last_refresh);
    Ok(Value::Object(staged))
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, WikiRuntimeFailure> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .ok_or(WikiRuntimeFailure::InvalidAuthentication)?;
    if value.is_empty()
        || value.len() > CREDENTIAL_VALUE_LIMIT
        || value != value.trim()
        || value.chars().any(char::is_control)
    {
        return Err(WikiRuntimeFailure::InvalidAuthentication);
    }
    Ok(value)
}

fn validate_access_token_expiry(token: &str) -> Result<(), WikiRuntimeFailure> {
    let mut parts = token.split('.');
    let header = parts.next();
    let payload = parts.next();
    let signature = parts.next();
    if header.is_none()
        || payload.is_none()
        || signature.is_none()
        || parts.next().is_some()
        || header.is_some_and(str::is_empty)
        || payload.is_some_and(str::is_empty)
        || signature.is_some_and(str::is_empty)
    {
        return Err(WikiRuntimeFailure::InvalidAuthentication);
    }
    let payload = payload.ok_or(WikiRuntimeFailure::InvalidAuthentication)?;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| WikiRuntimeFailure::InvalidAuthentication)?;
    let payload: Value =
        serde_json::from_slice(&payload).map_err(|_| WikiRuntimeFailure::InvalidAuthentication)?;
    let exp = payload
        .get("exp")
        .and_then(Value::as_i64)
        .ok_or(WikiRuntimeFailure::InvalidAuthentication)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WikiRuntimeFailure::InvalidAuthentication)?
        .as_secs();
    if exp <= now as i64 {
        return Err(WikiRuntimeFailure::ExpiredAuthentication);
    }
    Ok(())
}

fn write_private_json(destination: &Path, value: Value) -> Result<(), WikiRuntimeFailure> {
    let parent = destination
        .parent()
        .ok_or(WikiRuntimeFailure::InvalidStateDirectory)?;
    let metadata =
        fs::symlink_metadata(parent).map_err(|_| WikiRuntimeFailure::InvalidStateDirectory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(WikiRuntimeFailure::InvalidStateDirectory);
    }
    let bytes =
        serde_json::to_vec(&value).map_err(|_| WikiRuntimeFailure::InvalidAuthentication)?;
    if bytes.len() > CODEX_AUTH_FILE_LIMIT as usize {
        return Err(WikiRuntimeFailure::AuthenticationLimit);
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(destination)
        .map_err(|_| WikiRuntimeFailure::InvalidStateDirectory)?;
    let result = file
        .write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| WikiRuntimeFailure::InvalidStateDirectory);
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

fn read_claude_access_token(cancelled: &AtomicBool) -> Result<String, WikiRuntimeFailure> {
    #[cfg(all(feature = "system-keyring", target_os = "macos"))]
    {
        let account =
            std::env::var("USER").map_err(|_| WikiRuntimeFailure::AuthenticationUnavailable)?;
        if account.is_empty()
            || account.len() > CREDENTIAL_VALUE_LIMIT
            || account != account.trim()
            || account.chars().any(char::is_control)
        {
            return Err(WikiRuntimeFailure::AuthenticationUnavailable);
        }
        // Avoid keyring::Entry::get_password here: its native
        // SecKeychainFindGenericPassword call can wait indefinitely for
        // SecurityAgent. The stable security CLI is run through the bounded
        // child runner below, and its captured streams never enter a log.
        let mut command = Command::new("/usr/bin/security");
        command.args([
            "find-generic-password",
            "-s",
            CLAUDE_KEYCHAIN_SERVICE,
            "-a",
            &account,
            "-w",
        ]);
        read_claude_credentials_with_command(command, cancelled, claude_auth_policy())
    }
    #[cfg(not(all(feature = "system-keyring", target_os = "macos")))]
    {
        let _ = cancelled;
        Err(WikiRuntimeFailure::AuthenticationUnavailable)
    }
}

#[cfg(all(feature = "system-keyring", target_os = "macos"))]
fn claude_auth_policy() -> BoundedPolicy {
    BoundedPolicy {
        timeout: CLAUDE_AUTH_TIMEOUT,
        budget: OutputBudget::PerStream {
            stdout: CLAUDE_CREDENTIALS_LIMIT as u64,
            stderr: CLAUDE_AUTH_STDERR_LIMIT,
        },
    }
}

#[cfg(any(test, all(feature = "system-keyring", target_os = "macos")))]
fn read_claude_credentials_with_command(
    mut command: Command,
    cancelled: &AtomicBool,
    policy: BoundedPolicy,
) -> Result<String, WikiRuntimeFailure> {
    command.stdin(Stdio::null());
    let output = output_with_policy(command, policy, cancelled)
        .map_err(map_claude_process_failure)?
        .output;
    if !output.status.success() {
        return Err(WikiRuntimeFailure::AuthenticationUnavailable);
    }
    parse_claude_credentials(&output.stdout)
}

#[cfg(any(test, all(feature = "system-keyring", target_os = "macos")))]
fn map_claude_process_failure(failure: BoundedFailure) -> WikiRuntimeFailure {
    match failure {
        BoundedFailure::AggregateLimit
        | BoundedFailure::StdoutLimit
        | BoundedFailure::StderrLimit => WikiRuntimeFailure::AuthenticationLimit,
        failure => WikiRuntimeFailure::Process(failure),
    }
}

#[cfg(any(test, all(feature = "system-keyring", target_os = "macos")))]
fn parse_claude_credentials(raw: &[u8]) -> Result<String, WikiRuntimeFailure> {
    if raw.len() > CLAUDE_CREDENTIALS_LIMIT {
        return Err(WikiRuntimeFailure::AuthenticationLimit);
    }
    let value: Value =
        serde_json::from_slice(raw).map_err(|_| WikiRuntimeFailure::InvalidAuthentication)?;
    let oauth = value
        .get("claudeAiOauth")
        .and_then(Value::as_object)
        .ok_or(WikiRuntimeFailure::InvalidAuthentication)?;
    let access_token = oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .ok_or(WikiRuntimeFailure::InvalidAuthentication)?;
    if access_token.is_empty()
        || access_token.len() > CREDENTIAL_VALUE_LIMIT
        || access_token != access_token.trim()
        || access_token.chars().any(char::is_control)
    {
        return Err(WikiRuntimeFailure::InvalidAuthentication);
    }
    let expires_at = oauth
        .get("expiresAt")
        .and_then(Value::as_i64)
        .ok_or(WikiRuntimeFailure::InvalidAuthentication)?;
    let now_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WikiRuntimeFailure::InvalidAuthentication)?
        .as_millis();
    if expires_at <= now_millis as i64 {
        return Err(WikiRuntimeFailure::ExpiredAuthentication);
    }
    Ok(access_token.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    use std::process::Command;
    #[cfg(unix)]
    use std::time::{Duration, Instant};

    fn jwt(exp: i64) -> String {
        let payload = serde_json::json!({"exp": exp});
        format!(
            "e30.{}.sig",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string())
        )
    }

    fn codex_source(exp: i64) -> Vec<u8> {
        serde_json::json!({
            "OPENAI_API_KEY": "api-key-that-must-not-be-staged",
            "tokens": {
                "access_token": jwt(exp),
                "id_token": "id-token",
                "account_id": "account-id",
                "refresh_token": "refresh-token-that-must-not-be-staged"
            },
            "last_refresh": "2026-09-14T00:00:00Z"
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn codex_missing_required_credentials_fails_closed() {
        let source = br#"{"tokens":{"access_token":"x","id_token":"id","account_id":"account"},"last_refresh":"now"}"#;
        let fixture = tempfile::tempdir().expect("temp");
        let destination = fixture.path().join("auth.json");
        assert_eq!(
            stage_codex_auth_bytes(source, &destination),
            Err(WikiRuntimeFailure::InvalidAuthentication)
        );
        assert!(!destination.exists());
    }

    #[test]
    fn codex_expired_access_token_is_rejected() {
        let source = codex_source(0);
        let fixture = tempfile::tempdir().expect("temp");
        let destination = fixture.path().join("auth.json");
        assert_eq!(
            stage_codex_auth_bytes(&source, &destination),
            Err(WikiRuntimeFailure::ExpiredAuthentication)
        );
        assert!(!destination.exists());
    }

    #[cfg(unix)]
    #[test]
    fn bounded_codex_reader_rejects_missing_oversized_symlink_and_fifo() {
        let fixture = tempfile::tempdir().expect("temp");
        let missing = fixture.path().join("missing-auth.json");
        assert_eq!(
            read_bounded_file(&missing, CODEX_AUTH_FILE_LIMIT),
            Err(WikiRuntimeFailure::AuthenticationUnavailable)
        );

        let oversized = fixture.path().join("oversized-auth.json");
        fs::write(&oversized, vec![b'x'; CODEX_AUTH_FILE_LIMIT as usize + 1])
            .expect("oversized fixture");
        assert_eq!(
            read_bounded_file(&oversized, CODEX_AUTH_FILE_LIMIT),
            Err(WikiRuntimeFailure::AuthenticationLimit)
        );

        let regular = fixture.path().join("regular-auth.json");
        fs::write(&regular, b"{}").expect("regular fixture");
        let symlink = fixture.path().join("symlink-auth.json");
        std::os::unix::fs::symlink(&regular, &symlink).expect("symlink fixture");
        assert_eq!(
            read_bounded_file(&symlink, CODEX_AUTH_FILE_LIMIT),
            Err(WikiRuntimeFailure::AuthenticationUnavailable)
        );

        let fifo = fixture.path().join("fifo-auth.json");
        let status = Command::new("/usr/bin/mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo");
        assert!(status.success(), "mkfifo failed: {status:?}");
        let started = Instant::now();
        assert_eq!(
            read_bounded_file(&fifo, CODEX_AUTH_FILE_LIMIT),
            Err(WikiRuntimeFailure::AuthenticationUnavailable)
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "FIFO auth read blocked"
        );
    }

    #[test]
    fn codex_staging_omits_api_and_refresh_credentials_and_uses_private_file() {
        let source = codex_source(i64::MAX);
        let source_before = source.clone();
        let fixture = tempfile::tempdir().expect("temp");
        let destination = fixture.path().join("auth.json");
        stage_codex_auth_bytes(&source, &destination).expect("staged auth");
        let staged = fs::read_to_string(&destination).expect("staged bytes");
        assert!(!staged.contains("OPENAI_API_KEY"));
        assert!(!staged.contains("refresh-token-that-must-not-be-staged"));
        assert!(staged.contains(r#""refresh_token":""#));
        assert!(staged.contains("2026-09-14T00:00:00Z"));
        assert_eq!(source, source_before, "source bytes are not rewritten");
        #[cfg(unix)]
        {
            let mode = fs::metadata(&destination)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(String::from_utf8_lossy(&source).contains("refresh-token-that-must-not-be-staged"));
    }

    #[test]
    fn claude_missing_and_expired_credentials_fail_closed() {
        assert_eq!(
            parse_claude_credentials(br#"{}"#),
            Err(WikiRuntimeFailure::InvalidAuthentication)
        );
        let expired = serde_json::json!({
            "claudeAiOauth": {"accessToken":"access", "expiresAt":0, "refreshToken":"refresh"}
        });
        assert_eq!(
            parse_claude_credentials(expired.to_string().as_bytes()),
            Err(WikiRuntimeFailure::ExpiredAuthentication)
        );
    }

    #[test]
    fn claude_parser_returns_only_unexpired_access_token() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis() as i64;
        let credentials = serde_json::json!({
            "claudeAiOauth": {
                "accessToken":"access-token",
                "refreshToken":"refresh-token-that-must-not-be-forwarded",
                "expiresAt": now + 60_000
            }
        });
        let token = parse_claude_credentials(credentials.to_string().as_bytes()).expect("token");
        assert_eq!(token, "access-token");
        assert!(!token.contains("refresh-token"));
    }

    #[cfg(unix)]
    fn fake_security(script: &str) -> (tempfile::TempDir, PathBuf) {
        let fixture = tempfile::tempdir().expect("temp");
        let executable = fixture.path().join("security");
        fs::write(&executable, script).expect("fake security");
        let mut permissions = fs::metadata(&executable)
            .expect("fake metadata")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&executable, permissions).expect("fake permissions");
        (fixture, executable)
    }

    /// Timeout for the cases that are NOT about the deadline. It has to be
    /// generous: a short bound turns ordinary machine load into a deadline
    /// failure and hides the outcome these tests actually assert. The deadline
    /// itself has its own test with its own short bound.
    #[cfg(unix)]
    const NON_DEADLINE_TIMEOUT: Duration = Duration::from_secs(60);

    #[cfg(unix)]
    fn fake_claude_policy(timeout: Duration, stdout: u64) -> BoundedPolicy {
        BoundedPolicy {
            timeout,
            budget: OutputBudget::PerStream {
                stdout,
                stderr: 1024,
            },
        }
    }

    #[cfg(unix)]
    fn unexpired_claude_json() -> String {
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis() as i64
            + 60_000;
        serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "synthetic-access-token",
                "expiresAt": expires_at
            }
        })
        .to_string()
    }

    #[cfg(unix)]
    #[test]
    fn claude_credential_process_uses_the_production_bounded_reader() {
        let payload = unexpired_claude_json();
        let script = format!("#!/bin/sh\nprintf '%s' '{payload}'\n");
        let (_fixture, executable) = fake_security(&script);
        let token = read_claude_credentials_with_command(
            Command::new(executable),
            &AtomicBool::new(false),
            fake_claude_policy(NON_DEADLINE_TIMEOUT, CLAUDE_CREDENTIALS_LIMIT as u64),
        )
        .expect("synthetic credentials");
        assert_eq!(token, "synthetic-access-token");
    }

    #[cfg(unix)]
    #[test]
    fn claude_credential_process_timeout_is_bounded_and_redacted() {
        let (_fixture, executable) = fake_security("#!/bin/sh\nexec sleep 30\n");
        let started = Instant::now();
        let error = read_claude_credentials_with_command(
            Command::new(executable),
            &AtomicBool::new(false),
            fake_claude_policy(Duration::from_millis(50), 1024),
        )
        .expect_err("hung keychain helper must fail");
        assert_eq!(error, WikiRuntimeFailure::Process(BoundedFailure::Deadline));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "credential helper hung"
        );
        assert!(!format!("{error:?}").contains("synthetic"));
    }

    #[cfg(unix)]
    #[test]
    fn claude_credential_process_honors_cancellation_before_spawn() {
        let (_fixture, executable) = fake_security("#!/bin/sh\nexit 0\n");
        let cancelled = AtomicBool::new(true);
        let error = read_claude_credentials_with_command(
            Command::new(executable),
            &cancelled,
            fake_claude_policy(NON_DEADLINE_TIMEOUT, 1024),
        )
        .expect_err("cancelled credential helper must fail");
        assert_eq!(
            error,
            WikiRuntimeFailure::Process(BoundedFailure::Cancelled)
        );
    }

    #[cfg(unix)]
    #[test]
    fn claude_credential_process_nonzero_exit_does_not_parse_or_log_output() {
        let (_fixture, executable) = fake_security(
            "#!/bin/sh\nprintf 'SECRET_CREDENTIAL_OUTPUT'\nprintf 'SECRET_DIAGNOSTIC' >&2\nexit 23\n",
        );
        let error = read_claude_credentials_with_command(
            Command::new(executable),
            &AtomicBool::new(false),
            fake_claude_policy(NON_DEADLINE_TIMEOUT, 1024),
        )
        .expect_err("failed keychain helper must fail");
        assert_eq!(error, WikiRuntimeFailure::AuthenticationUnavailable);
        let diagnostics = format!("{error:?}");
        assert!(!diagnostics.contains("SECRET_CREDENTIAL_OUTPUT"));
        assert!(!diagnostics.contains("SECRET_DIAGNOSTIC"));
    }

    #[cfg(unix)]
    #[test]
    fn claude_credential_process_oversized_output_is_rejected_before_parse() {
        let (_fixture, executable) = fake_security("#!/bin/sh\nprintf '%2048s' x\n");
        let error = read_claude_credentials_with_command(
            Command::new(executable),
            &AtomicBool::new(false),
            fake_claude_policy(NON_DEADLINE_TIMEOUT, 1024),
        )
        .expect_err("oversized credential output must fail");
        assert_eq!(error, WikiRuntimeFailure::AuthenticationLimit);
    }
}
