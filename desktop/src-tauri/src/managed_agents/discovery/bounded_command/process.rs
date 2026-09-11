//! Existing process-group / Job Object ownership, with bounded checked cleanup.
#[cfg(windows)]
use super::BOUNDED_CREATION_FLAGS;
#[cfg(unix)]
use super::KILL_GRACE;
use super::{BoundedFailure, CLEANUP_BUDGET, POLL_INTERVAL};
use std::process::{ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus};
use std::time::Instant;

/// A spawned child plus ownership of its descendant tree, torn down on *every*
/// exit path — timeout, error, or successful exit. The two platforms establish
/// ownership differently, and the guarantee is deliberately asymmetric — the
/// adjudicated design, not an oversight:
///
/// - **Unix:** the child leads its own process group (`process_group(0)`), so
///   `killpg` reaches every descendant that has not left the group. A
///   `setsid`/`setpgid` escapee holding a pipe is *not* owned and may survive
///   one probe, yet never hangs the helper (see [`super::output_with_timeout`]).
/// - **Windows:** the child is spawned `CREATE_SUSPENDED`, assigned to a
///   kill-on-close Job Object while frozen, then resumed. The job owns the root
///   before any descendant can exist and is created without breakaway, so no
///   writer can escape it — a hard whole-tree guarantee. Closing that job reaps
///   the whole tree *even after the root has exited* — the distinction that
///   makes `taskkill /T <pid>` (a live-root lookup) unfit for the success path.
///   This mirrors the Job Object discipline the harness uses to reap its 24
///   agent workers (`process_lifecycle.rs`).
pub(super) struct BoundedChild {
    child: std::process::Child,
    #[cfg(unix)]
    group: OwnedProcessGroup,
    /// The kill-on-close job that owns the whole tree. Taken and dropped by
    /// `kill_tree` so the reap happens exactly once. Spawn is fail-closed: if
    /// the job cannot be created, assigned, or the child resumed, the child is
    /// terminated and `spawn` returns `None` rather than running unowned.
    #[cfg(windows)]
    job: Option<crate::managed_agents::JobHandle>,
}

/// Only a positive, child-created group can reach killpg; never zero or -1.
#[cfg(unix)]
struct OwnedProcessGroup(std::num::NonZeroI32);

#[cfg(unix)]
impl OwnedProcessGroup {
    fn new(child_id: u32) -> Option<Self> {
        let id = i32::try_from(child_id).ok()?;
        (id > 0)
            .then(|| std::num::NonZeroI32::new(id))
            .flatten()
            .map(Self)
    }
}

impl BoundedChild {
    /// Spawn `command`, establishing tree ownership before the child can run.
    /// Returns `None` if the spawn fails or — on Windows — if the job cannot be
    /// created, assigned, or the frozen child resumed; in every such case the
    /// child is terminated and reaped before returning, so no unowned process
    /// survives.
    pub(super) fn spawn(mut command: Command) -> Option<Self> {
        // Run the child in its own process group so the whole tree can be torn
        // down as a unit, not just a direct child that may have forked workers.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            command.process_group(0);
        }

        // Spawn frozen so the Job Object can take ownership before any child
        // code runs and forks a descendant that would escape the job. The flags
        // are set here as the last writer before spawn; `Command::creation_flags`
        // replaces rather than ORs, so `BOUNDED_CREATION_FLAGS` must itself carry
        // `CREATE_NO_WINDOW` — a caller's earlier `configure_no_window` would be
        // clobbered otherwise, flashing a console window on GUI discovery.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            command.creation_flags(BOUNDED_CREATION_FLAGS);
        }

        // `mut` is used only on the Windows fail-closed path (kill/wait on the
        // frozen child); Unix moves the child unmodified into `Self`.
        let mut child = command.spawn().ok()?;

        #[cfg(unix)]
        let group = match OwnedProcessGroup::new(child.id()) {
            Some(group) => group,
            None => {
                let _ = child.kill();
                let _ = reap_child_until(&mut child, Instant::now() + CLEANUP_BUDGET);
                return None;
            }
        };

        #[cfg(windows)]
        let job = {
            // Assign the frozen child to a kill-on-close job, then resume it.
            // Any failure is fail-closed: terminate + reap the still-owned
            // child and abort the spawn, never run it unowned to the deadline.
            let Some(job) = crate::managed_agents::create_job_for_child(child.id()) else {
                let _ = child.kill();
                let _ = reap_child_until(&mut child, Instant::now() + CLEANUP_BUDGET);
                return None;
            };
            if !crate::managed_agents::resume_process(child.id()) {
                // Dropping the job kills the still-suspended child via
                // kill-on-close; reap it so no zombie lingers.
                drop(job);
                let _ = reap_child_until(&mut child, Instant::now() + CLEANUP_BUDGET);
                return None;
            }
            job
        };

        Some(Self {
            child,
            #[cfg(unix)]
            group,
            #[cfg(windows)]
            job: Some(job),
        })
    }

    pub(super) fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Timeout teardown: a graceful `SIGTERM` to the group and a bounded grace
    /// period for a clean flush on Unix, then the unconditional forced kill.
    /// Windows has no group signal, so it goes straight to the forced kill.
    pub(super) fn terminate_timed_out(&mut self) -> Result<(), BoundedFailure> {
        #[cfg(unix)]
        {
            let signalled = self.signal_group(libc::SIGTERM);
            std::thread::sleep(KILL_GRACE);
            let killed = self.kill_tree();
            signalled.and(killed)
        }
        #[cfg(not(unix))]
        self.kill_tree()
    }

    /// Forcibly reap the whole tree. Idempotent and safe on an already-exited
    /// tree. Runs on every exit path — including success, because a login shell
    /// or auth CLI can background a descendant that outlives the leader while
    /// still holding the captured-output descriptors.
    pub(super) fn kill_tree(&mut self) -> Result<(), BoundedFailure> {
        #[cfg(unix)]
        let group_result = self.signal_group(libc::SIGKILL);
        #[cfg(not(unix))]
        let group_result = Ok(());
        #[cfg(windows)]
        // Closing the kill-on-close job reaps every descendant, even once the
        // root has exited — which `taskkill /T <root>` cannot. `spawn` is
        // fail-closed, so the job is always present until this first take;
        // a later take is a no-op (the tree is already reaped).
        if let Some(job) = self.job.take() {
            drop(job);
        }
        // A Unix root can move itself out of its original process group. The
        // direct Child remains ours, so terminate it independently as well.
        let root_result = match self.child.try_wait() {
            Ok(Some(_)) => Ok(()),
            Ok(None) => match self.child.kill() {
                Ok(()) => Ok(()),
                Err(_) => match self.child.try_wait() {
                    Ok(Some(_)) => Ok(()),
                    _ => Err(BoundedFailure::Cleanup),
                },
            },
            Err(_) => Err(BoundedFailure::Cleanup),
        };
        group_result.and(root_result)
    }

    #[cfg(unix)]
    fn signal_group(&mut self, signal: i32) -> Result<(), BoundedFailure> {
        let group = self.group.0.get();
        let send = || {
            // SAFETY: group is the private positive PGID established at spawn.
            // This is the existing killpg primitive, now with checked errors.
            if unsafe { libc::killpg(group, signal) } == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        };
        super::policy::signal_group_with_reap_retry(send, || {
            self.child.try_wait().map(|s| s.is_some())
        })
    }

    /// Poll-reap only; a stuck root must not turn a timeout into a blocking wait.
    pub(super) fn reap_until(&mut self, deadline: Instant) -> Result<(), BoundedFailure> {
        reap_child_until(&mut self.child, deadline)
    }

    /// Take the captured stdout pipe. `Some` because [`super::output_with_timeout`]
    /// configures `Stdio::piped()` before spawn.
    pub(super) fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    /// Take the child stdin when a bounded caller supplies a prompt payload.
    pub(super) fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }

    /// Take the captured stderr pipe.
    pub(super) fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }
}

fn reap_child_until(
    child: &mut std::process::Child,
    deadline: Instant,
) -> Result<(), BoundedFailure> {
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Err(_) => return Err(BoundedFailure::Cleanup),
            Ok(None) if Instant::now() >= deadline => return Err(BoundedFailure::Cleanup),
            Ok(None) => std::thread::sleep(
                POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
            ),
        }
    }
}
