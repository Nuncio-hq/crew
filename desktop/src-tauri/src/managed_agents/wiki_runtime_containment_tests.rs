use super::*;

#[cfg(target_os = "macos")]
fn claude_default() -> WikiRuntimeSelection {
    WikiRuntimeSelection {
        runtime_id: "claude".into(),
        model: None,
        profile: None,
    }
}

#[cfg(target_os = "macos")]
fn fake_runtime(script: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("fixture dir");
    let executable = dir.path().join("fake-runtime");
    std::fs::write(&executable, script).expect("script");
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
        .expect("permissions");
    (dir, executable)
}

/// Exercise the command builder used by the installed Wiki invocation. The
/// fake runtime tries to fork a child writer; the fixed Seatbelt wrapper must
/// deny that fork before the writer can create its marker. If the wrapper is
/// removed, the marker is created and this test fails.
#[cfg(target_os = "macos")]
#[test]
fn installed_wiki_command_builder_denies_runtime_fork() {
    let (fixture, executable) = fake_runtime(
        r#"#!/usr/bin/perl
use strict;
use warnings;
my $pid = fork();
die "fork failed: $!" unless defined $pid;
if ($pid == 0) {
    open my $marker, '>', 'forked' or die "marker: $!";
    print {$marker} "forked";
    close $marker;
    exit 0;
}
waitpid($pid, 0);
"#,
    );
    let state = fixture.path().join("state");
    let mut generator =
        WikiRuntimeGenerator::with_executable(claude_default(), executable.clone(), state.clone())
            .expect("generator");
    generator.sandbox_profile =
        native_containment_profile(&executable).expect("macOS containment profile");
    let command = generator.command(Some("prompt"));
    let program = command.get_program().to_owned();
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    // Prove that the fixture itself creates the marker when it is not wrapped
    // by the production containment command. The child waits and is reaped,
    // so this control leaves no process behind.
    let mut control_command = std::process::Command::new(&executable);
    control_command.env_clear().current_dir(&state);
    let control_cancelled = std::sync::atomic::AtomicBool::new(false);
    let control = output_with_policy(
        control_command,
        BoundedPolicy {
            timeout: std::time::Duration::from_secs(2),
            budget: OutputBudget::PerStream {
                stdout: 1024,
                stderr: 1024,
            },
        },
        &control_cancelled,
    )
    .expect("uncontained control runtime");
    assert!(control.output.status.success());
    assert_eq!(
        std::fs::read_to_string(state.join("forked")).expect("control marker"),
        "forked"
    );
    std::fs::remove_file(state.join("forked")).expect("remove control marker");

    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let outcome = output_with_policy(
        command,
        BoundedPolicy {
            timeout: std::time::Duration::from_secs(2),
            budget: OutputBudget::PerStream {
                stdout: 1024,
                stderr: 1024,
            },
        },
        &cancelled,
    )
    .expect("sandbox denial should still produce bounded child output");
    assert!(!outcome.output.status.success());
    let stderr = String::from_utf8_lossy(&outcome.output.stderr);
    assert!(
        stderr.contains("fork failed") || stderr.contains("Operation not permitted"),
        "expected fork denial, stderr was {stderr:?}"
    );
    assert!(!state.join("forked").exists());
    assert_eq!(program, std::ffi::OsStr::new("/usr/bin/sandbox-exec"));
    assert_eq!(args.first().map(String::as_str), Some("-p"));
    assert_eq!(
        args.get(1).map(String::as_str),
        Some("(version 1)(allow default)(deny process-fork)")
    );
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[test]
fn unsupported_unix_has_no_native_wiki_containment_profile() {
    assert_eq!(
        native_containment_profile(Path::new("/bin/sh")),
        Err(super::super::recap_adapter::RecapRunFailure::UnsupportedContainment)
    );
}

#[test]
fn installed_constructor_rejects_deferred_claude_before_runtime_setup() {
    let result = WikiRuntimeGenerator::installed_with_cancel(
        WikiRuntimeSelection {
            runtime_id: "claude".into(),
            model: Some("claude-fable-5-1".into()),
            profile: None,
        },
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );

    assert_eq!(
        result.err(),
        Some(WikiRuntimeFailure::UnsupportedRuntime("claude".into()))
    );
}
