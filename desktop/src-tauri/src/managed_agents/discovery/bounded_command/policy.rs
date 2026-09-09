use std::process::Output;
use std::time::Duration;

/// Discovery keeps its original aggregate cap; recap caps each stream.
#[derive(Debug, Clone, Copy)]
pub(crate) enum OutputBudget {
    Aggregate(u64),
    #[allow(dead_code)] // Default-off recap proof; discovery retains Aggregate.
    PerStream {
        stdout: u64,
        stderr: u64,
    },
}

/// Wall-clock and capture policy. Poll intervals are fixed by the runner.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BoundedPolicy {
    pub timeout: Duration,
    pub budget: OutputBudget,
}

/// Fixed diagnostics; raw provider text is never embedded in a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoundedFailure {
    Cancelled,
    Deadline,
    AggregateLimit,
    StdoutLimit,
    StderrLimit,
    ProcessOwnership,
    Pipe,
    Read,
    Wait,
    Cleanup,
    InvalidBounds,
}

/// Captured process result, not a certificate that all descendants exited.
pub(crate) struct BoundedOutcome {
    pub output: Output,
}

impl std::fmt::Debug for BoundedOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundedOutcome")
            .field("status", &self.output.status)
            .field("stdout_bytes", &self.output.stdout.len())
            .field("stderr_bytes", &self.output.stderr.len())
            .finish()
    }
}

/// EPERM alone is not a zombie diagnosis. Retry once only after observed reap.
#[cfg(unix)]
pub(super) fn signal_group_with_reap_retry(
    mut signal: impl FnMut() -> std::io::Result<()>,
    mut root_reaped: impl FnMut() -> std::io::Result<bool>,
) -> Result<(), BoundedFailure> {
    match signal() {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(libc::ESRCH) => Ok(()),
        Err(error) if error.raw_os_error() == Some(libc::EPERM) => {
            if !root_reaped().map_err(|_| BoundedFailure::Cleanup)? {
                return Err(BoundedFailure::Cleanup);
            }
            match signal() {
                Ok(()) => Ok(()),
                Err(error) if error.raw_os_error() == Some(libc::ESRCH) => Ok(()),
                Err(_) => Err(BoundedFailure::Cleanup),
            }
        }
        Err(_) => Err(BoundedFailure::Cleanup),
    }
}
