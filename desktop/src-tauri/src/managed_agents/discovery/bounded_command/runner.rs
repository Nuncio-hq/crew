//! Capture policy atop the existing process ownership primitive.
//!
//! Success reaps the exited root before final group cleanup. Timeout/cancel
//! terminates first and only then polls for root exit. Unix escaped descendants
//! remain outside the ownership boundary; stopping their pipe readers is not
//! evidence that those descendants died.

use super::*;

/// Run with configured stdin, fixed polling, cancellation and bounded capture.
pub(crate) fn output_with_policy(
    mut command: Command,
    policy: BoundedPolicy,
    cancelled: &AtomicBool,
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
    let (stdout_limit, stderr_limit, aggregate) = match policy.budget {
        OutputBudget::Aggregate(limit) => (limit, limit, true),
        OutputBudget::PerStream { stdout, stderr } => (stdout, stderr, false),
    };
    // This helper is not a way to expand discovery's existing memory ceiling.
    if stdout_limit > CAPTURE_LIMIT || stderr_limit > CAPTURE_LIMIT {
        return Err(BoundedFailure::InvalidBounds);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = BoundedChild::spawn(command).ok_or(BoundedFailure::ProcessOwnership)?;
    let stdout_pipe = child.take_stdout();
    let stderr_pipe = child.take_stderr();
    let pipes_valid = stdout_pipe.is_some() && stderr_pipe.is_some();
    #[cfg(unix)]
    let pipes_valid = pipes_valid
        && stdout_pipe.as_ref().is_some_and(set_nonblocking)
        && stderr_pipe.as_ref().is_some_and(set_nonblocking);
    if !pipes_valid {
        let killed = child.kill_tree();
        let reaped = child.reap_until(Instant::now() + CLEANUP_BUDGET);
        killed.and(reaped)?;
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
    termination.and(killed).and(reaped)?;
    let stdout = stdout?;
    let stderr = stderr?;
    if let Some(failure) = overflow() {
        return Err(failure);
    }
    let status = status?;
    Ok(BoundedOutcome {
        output: Output {
            status,
            stdout,
            stderr,
        },
    })
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
