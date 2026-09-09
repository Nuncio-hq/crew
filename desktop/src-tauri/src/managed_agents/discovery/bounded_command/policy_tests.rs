#![cfg(unix)]

use super::*;

fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script]);
    command
}

fn per_stream(bytes: u64) -> BoundedPolicy {
    BoundedPolicy {
        timeout: Duration::from_secs(2),
        budget: OutputBudget::PerStream {
            stdout: bytes,
            stderr: bytes,
        },
    }
}

#[test]
fn configured_stdin_handle_reaches_child() {
    let input = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(input.path(), b"synthetic stdin").unwrap();
    let file = std::fs::File::open(input.path()).unwrap();
    let mut command = shell("cat");
    command.stdin(Stdio::from(file));
    let result = output_with_policy(command, per_stream(64), &AtomicBool::new(false)).unwrap();
    assert_eq!(result.output.stdout, b"synthetic stdin");
}

#[test]
fn each_stream_has_its_own_enforced_budget() {
    for (script, failure) in [
        ("head -c 65 /dev/zero", BoundedFailure::StdoutLimit),
        ("head -c 65 /dev/zero >&2", BoundedFailure::StderrLimit),
    ] {
        assert_eq!(
            output_with_policy(shell(script), per_stream(64), &AtomicBool::new(false)).unwrap_err(),
            failure
        );
    }
}

#[test]
fn aggregate_discovery_budget_is_not_two_per_stream_budgets() {
    let script = "head -c 64 /dev/zero; head -c 64 /dev/zero >&2";
    let result =
        output_with_policy(shell(script), per_stream(64), &AtomicBool::new(false)).unwrap();
    assert_eq!(
        (result.output.stdout.len(), result.output.stderr.len()),
        (64, 64)
    );
    let policy = BoundedPolicy {
        budget: OutputBudget::Aggregate(64),
        ..per_stream(64)
    };
    assert_eq!(
        output_with_policy(shell(script), policy, &AtomicBool::new(false)).unwrap_err(),
        BoundedFailure::AggregateLimit
    );
}

#[test]
fn zero_deadline_and_precancel_never_spawn() {
    let root = tempfile::tempdir().unwrap();
    for (name, timeout, cancelled, failure) in [
        ("zero", Duration::ZERO, false, BoundedFailure::Deadline),
        (
            "cancel",
            Duration::from_secs(2),
            true,
            BoundedFailure::Cancelled,
        ),
    ] {
        let sentinel = root.path().join(name);
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf spawned > \"$1\"", "fixture"])
            .arg(&sentinel);
        let result = output_with_policy(
            command,
            BoundedPolicy {
                timeout,
                ..per_stream(64)
            },
            &AtomicBool::new(cancelled),
        );
        assert_eq!(result.unwrap_err(), failure);
        assert!(
            !sentinel.exists(),
            "pre-spawn admission must not run the child"
        );
        // A failed exec observes a spawn attempt even if immediate cancellation
        // would kill a valid child before its sentinel instruction could run.
        let missing = Command::new(root.path().join("does-not-exist"));
        assert_eq!(
            output_with_policy(
                missing,
                BoundedPolicy {
                    timeout,
                    ..per_stream(64)
                },
                &AtomicBool::new(cancelled)
            )
            .unwrap_err(),
            failure
        );
    }
}

#[test]
fn cancellation_after_spawn_terminates_the_owned_process() {
    let root = tempfile::tempdir().unwrap();
    let ready = root.path().join("ready");
    let cancelled = Arc::new(AtomicBool::new(false));
    let child_cancelled = cancelled.clone();
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "printf '%s' \"$$\" > \"$1\"; exec sleep 30",
            "fixture",
        ])
        .arg(&ready);
    let worker = std::thread::spawn(move || {
        output_with_policy(
            command,
            BoundedPolicy {
                timeout: Duration::from_secs(30),
                ..per_stream(64)
            },
            &child_cancelled,
        )
    });
    let ready_deadline = Instant::now() + Duration::from_secs(2);
    while !ready.exists() && Instant::now() < ready_deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        ready.exists(),
        "healthy fake must start before cancellation"
    );
    cancelled.store(true, Ordering::Relaxed);
    let finish_deadline = Instant::now() + Duration::from_secs(2);
    while !worker.is_finished() && Instant::now() < finish_deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let finished = worker.is_finished();
    if !finished {
        // Preserve a healthy RED without leaving the deliberately uncancelled
        // fake behind. This PID came only from this test's private ready file.
        let pid: u32 = std::fs::read_to_string(&ready).unwrap().parse().unwrap();
        assert!(pid > 0);
        let status = Command::new("/bin/kill")
            .args(["-KILL", &pid.to_string()])
            .status()
            .unwrap();
        assert!(status.success(), "owned fixture cleanup must succeed");
    }
    let outcome = worker.join().unwrap();
    assert!(finished, "cancel must bound the running process");
    assert_eq!(outcome.unwrap_err(), BoundedFailure::Cancelled);
}

#[test]
fn outcome_debug_never_exposes_provider_streams() {
    let result = output_with_policy(
        shell("printf SECRET_STDOUT; printf SECRET_STDERR >&2; exit 7"),
        per_stream(64),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(result.output.status.code(), Some(7));
    let debug = format!("{result:?}");
    assert!(!debug.contains("SECRET_STDOUT"));
    assert!(!debug.contains("SECRET_STDERR"));
}

#[test]
fn eperm_retries_only_after_observed_root_reap() {
    use std::cell::Cell;
    let signals = Cell::new(0);
    let result = policy::signal_group_with_reap_retry(
        || {
            signals.set(signals.get() + 1);
            Err(std::io::Error::from_raw_os_error(libc::EPERM))
        },
        || Ok(false),
    );
    assert_eq!(result, Err(BoundedFailure::Cleanup));
    assert_eq!(signals.get(), 1, "live EPERM is not evidence of a zombie");
    signals.set(0);
    let result = policy::signal_group_with_reap_retry(
        || {
            signals.set(signals.get() + 1);
            Err(std::io::Error::from_raw_os_error(if signals.get() == 1 {
                libc::EPERM
            } else {
                libc::ESRCH
            }))
        },
        || Ok(true),
    );
    assert_eq!(result, Ok(()));
    assert_eq!(signals.get(), 2);
}

#[test]
fn persistent_eperm_and_reap_failure_are_cleanup_errors() {
    for reaped in [Ok(true), Err(std::io::Error::other("fixture"))] {
        let mut reaped = Some(reaped);
        assert_eq!(
            policy::signal_group_with_reap_retry(
                || Err(std::io::Error::from_raw_os_error(libc::EPERM)),
                || reaped.take().unwrap()
            ),
            Err(BoundedFailure::Cleanup)
        );
    }
}

#[test]
fn root_reap_deadline_is_bounded_even_while_root_is_alive() {
    let mut child = BoundedChild::spawn(shell("exec sleep 0.2")).unwrap();
    let result = child.reap_until(Instant::now());
    child.kill_tree().unwrap();
    child
        .reap_until(Instant::now() + Duration::from_secs(2))
        .unwrap();
    assert_eq!(result, Err(BoundedFailure::Cleanup));
}
