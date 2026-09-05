//! Runtime CLI executable environment overrides.
use super::{resolve_command, should_skip_claude_executable, KnownAcpRuntime};

pub(crate) fn configure_runtime_cli(
    command: &mut std::process::Command,
    runtime: Option<&KnownAcpRuntime>,
) {
    let Some(runtime) = runtime else {
        return;
    };
    if runtime.id != "claude" {
        return;
    }
    if let Some(cli_path) = runtime.underlying_cli.and_then(resolve_command) {
        // On Windows, `.cmd` and `.bat` files are batch shims — they cannot be
        // passed directly to `CreateProcess` and cause EINVAL when the Claude
        // adapter tries to spawn them (issue #2397). Skip setting
        // `CLAUDE_CODE_EXECUTABLE` for shim paths so the adapter falls back to
        // its own PATH lookup and finds the real binary instead.
        // Non-Windows: `.cmd`/`.bat` are valid executables and must be assigned.
        if should_skip_claude_executable(&cli_path, cfg!(windows)) {
            return;
        }
        command.env("CLAUDE_CODE_EXECUTABLE", cli_path);
    }
}
