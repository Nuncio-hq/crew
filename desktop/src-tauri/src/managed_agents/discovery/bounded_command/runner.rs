//! Capture policy atop the existing process ownership primitive.
//!
//! Success reaps the exited root before final group cleanup. Timeout/cancel
//! terminates first and only then polls for root exit. Unix escaped descendants
//! remain outside the ownership boundary; stopping their pipe readers is not
//! evidence that those descendants died.

use super::*;

/// Run while preserving the caller's stdin configuration, with fixed polling,
/// cancellation and bounded capture.
pub(crate) fn output_with_policy(
    command: Command,
    policy: BoundedPolicy,
    cancelled: &AtomicBool,
) -> Result<BoundedOutcome, BoundedFailure> {
    output_with_policy_and_stdin(command, None, policy, cancelled)
}

/// Run with an optional bounded stdin payload, fixed polling, cancellation and
/// bounded capture. The payload is written by a cancellation-aware helper
/// thread so a runtime that never consumes stdin still reaches the process
/// deadline and cleanup path.
pub(crate) fn output_with_policy_and_stdin(
    command: Command,
    input: Option<Vec<u8>>,
    policy: BoundedPolicy,
    cancelled: &AtomicBool,
) -> Result<BoundedOutcome, BoundedFailure> {
    output_with_policy_and_stdin_and_spawn_hook(command, input, policy, cancelled, |_| Ok(()))
}

/// Run a bounded child and invoke `on_spawn` after tree ownership is secured.
///
/// The hook is used by durable callers to persist the owned child identity
/// before any output is consumed. A hook failure tears down the owned tree and
/// returns `Cleanup`; callers retain their pre-spawn durable retry record.
pub(crate) fn output_with_policy_and_spawn_hook(
    command: Command,
    policy: BoundedPolicy,
    cancelled: &AtomicBool,
    on_spawn: impl FnOnce(u32) -> Result<(), BoundedFailure>,
) -> Result<BoundedOutcome, BoundedFailure> {
    output_with_policy_and_stdin_and_spawn_hook(command, None, policy, cancelled, on_spawn)
}

fn output_with_policy_and_stdin_and_spawn_hook(
    mut command: Command,
    input: Option<Vec<u8>>,
    policy: BoundedPolicy,
    cancelled: &AtomicBool,
    on_spawn: impl FnOnce(u32) -> Result<(), BoundedFailure>,
) -> Result<BoundedOutcome, BoundedFailure> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(BoundedFailure::Cancelled);
    }
    if policy.timeout.is_zero() {
        return Err(BoundedFailure::Deadline);
    }
    let deadline = Instant::now()
        .checked_add(policy.timeout)
        .ok_or(BoundedFailure::Deadline)?;
    let (stdout_limit, stderr_limit, aggregate, capture_limit) = match policy.budget {
        OutputBudget::Aggregate(limit) => (limit, limit, true, CAPTURE_LIMIT),
        OutputBudget::PerStream { stdout, stderr } => {
            (stdout, stderr, false, PER_STREAM_CAPTURE_LIMIT)
        }
    };
    if stdout_limit > capture_limit || stderr_limit > capture_limit {
        return Err(BoundedFailure::InvalidBounds);
    }
    if let Some(input) = input.as_ref() {
        // The caller owns its complete-input bound. This protects this shared
        // runner if a future caller forgets to apply its own request limit.
        if input.len() > PER_STREAM_CAPTURE_LIMIT as usize {
            return Err(BoundedFailure::InvalidBounds);
        }
        command.stdin(Stdio::piped());
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = BoundedChild::spawn(command).ok_or(BoundedFailure::ProcessOwnership)?;
    if on_spawn(child.id()).is_err() {
        let killed = child.kill_tree();
        let reaped = child.reap_until(Instant::now() + CLEANUP_BUDGET);
        let _ = killed.and(reaped);
        return Err(BoundedFailure::Cleanup);
    }
    let stdin_stop = Arc::new(AtomicBool::new(false));
    let stdin_writer = input.map(|input| {
        let stdin = child.take_stdin().ok_or(BoundedFailure::Pipe)?;
        Ok(spawn_stdin_writer(stdin, input, stdin_stop.clone()))
    });
    let stdin_writer = match stdin_writer {
        Some(Ok(writer)) => Some(writer),
        Some(Err(error)) => {
            let killed = child.kill_tree();
            let reaped = child.reap_until(Instant::now() + CLEANUP_BUDGET);
            killed.and(reaped)?;
            return Err(error);
        }
        None => None,
    };
    let stdout_pipe = child.take_stdout();
    let stderr_pipe = child.take_stderr();
    let pipes_valid = stdout_pipe.is_some() && stderr_pipe.is_some();
    #[cfg(unix)]
    let pipes_valid = pipes_valid
        && stdout_pipe.as_ref().is_some_and(set_nonblocking)
        && stderr_pipe.as_ref().is_some_and(set_nonblocking);
    if !pipes_valid {
        stdin_stop.store(true, Ordering::Relaxed);
        let killed = child.kill_tree();
        let reaped = child.reap_until(Instant::now() + CLEANUP_BUDGET);
        let stdin = join_stdin_writer(stdin_writer, Instant::now() + CLEANUP_BUDGET);
        killed.and(reaped)?;
        stdin?;
        return Err(BoundedFailure::Pipe);
    }
    let stdout_total = Arc::new(AtomicU64::new(0));
    let stderr_total = if aggregate {
        stdout_total.clone()
    } else {
        Arc::new(AtomicU64::new(0))
    };
    let stdout_overflow = Arc::new(AtomicBool::new(false));
    let stderr_overflow = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let stdout_drain = stdout_pipe.map(|pipe| {
        spawn_drain(
            pipe,
            stdout_total,
            stdout_overflow.clone(),
            stop.clone(),
            stdout_limit,
        )
    });
    let stderr_drain = stderr_pipe.map(|pipe| {
        spawn_drain(
            pipe,
            stderr_total,
            stderr_overflow.clone(),
            stop.clone(),
            stderr_limit,
        )
    });
    let overflow = || {
        if aggregate
            && (stdout_overflow.load(Ordering::Relaxed) || stderr_overflow.load(Ordering::Relaxed))
        {
            Some(BoundedFailure::AggregateLimit)
        } else if stdout_overflow.load(Ordering::Relaxed) {
            Some(BoundedFailure::StdoutLimit)
        } else if stderr_overflow.load(Ordering::Relaxed) {
            Some(BoundedFailure::StderrLimit)
        } else {
            None
        }
    };
    let status = loop {
        if cancelled.load(Ordering::Relaxed) {
            break Err(BoundedFailure::Cancelled);
        }
        if let Some(failure) = overflow() {
            break Err(failure);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() >= deadline => break Err(BoundedFailure::Deadline),
            Ok(None) => std::thread::sleep(
                POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
            ),
            Err(_) => break Err(BoundedFailure::Wait),
        }
    };
    let cleanup_deadline = Instant::now() + CLEANUP_BUDGET;
    stdin_stop.store(true, Ordering::Relaxed);
    let termination = if matches!(
        status,
        Err(BoundedFailure::Deadline | BoundedFailure::Cancelled)
    ) {
        child.terminate_timed_out()
    } else {
        Ok(())
    };
    let killed = child.kill_tree();
    let reaped = child.reap_until(cleanup_deadline);
    stop.store(true, Ordering::Relaxed);
    // Always finish both drains, including when another cleanup operation failed.
    let stdout = join_drain(stdout_drain, cleanup_deadline);
    let stderr = join_drain(stderr_drain, cleanup_deadline);
    let stdin = join_stdin_writer(stdin_writer, cleanup_deadline);
    termination.and(killed).and(reaped)?;
    let stdout = stdout?;
    let stderr = stderr?;
    if let Some(failure) = overflow() {
        return Err(failure);
    }
    let status = status?;
    if let Err(failure) = stdin {
        // A successful runtime may stop consuming a large prompt as soon as
        // it has enough input to answer. Its closed stdin is not a failed
        // generation; process ownership and the bounded output are already
        // established. Preserve every other writer failure.
        if !(status.success() && failure == BoundedFailure::Pipe) {
            return Err(failure);
        }
    }
    Ok(BoundedOutcome {
        output: Output {
            status,
            stdout,
            stderr,
        },
    })
}

fn join_stdin_writer(
    writer: Option<JoinHandle<std::io::Result<()>>>,
    deadline: Instant,
) -> Result<(), BoundedFailure> {
    let Some(writer) = writer else {
        return Ok(());
    };
    while !writer.is_finished() {
        if Instant::now() >= deadline {
            return Err(BoundedFailure::Cleanup);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    writer
        .join()
        .map_err(|_| BoundedFailure::Read)?
        .map_err(|_| BoundedFailure::Pipe)
}

fn join_drain(
    drain: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
    deadline: Instant,
) -> Result<Vec<u8>, BoundedFailure> {
    let drain = drain.ok_or(BoundedFailure::Pipe)?;
    while !drain.is_finished() {
        if Instant::now() >= deadline {
            return Err(BoundedFailure::Cleanup);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    drain
        .join()
        .map_err(|_| BoundedFailure::Read)?
        .map_err(|_| BoundedFailure::Read)
}
