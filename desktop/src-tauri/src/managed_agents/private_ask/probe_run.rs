//! The production capability probe: one real run under the exact production
//! envelope, from which one [`PrivateAskProbe`] is captured.
//!
//! Nothing here reports on itself. The probe program's marker carries only the
//! *attempts* it made — the return value of its own syscall is the one thing
//! only it can see — and every *effect* is measured from this side:
//!
//! * the write outside the run root is judged by digesting a sentinel file this
//!   process owns, before and after the whole run;
//! * the direct connect is counted by a listener this process owns;
//! * the egress bound comes from the attempt proxy's own record;
//! * a surviving descendant is detected by taking an exclusive lock a forked
//!   descendant would still be holding — a liveness observation that needs no
//!   PID, so a recycled PID cannot answer for a process that already exited.
//!
//! The policy text is built by the same production function the launch plan
//! uses, from the *selected runtime's* directory and read roots, so it is
//! byte-identical to the text a real answer would run under. That is what
//! [`super::capability::PrivateAskCapability::from_probe`] rebuilds and
//! compares; a probe captured under anything weaker cannot certify this recipe.

use super::capability::{
    PrivateAskProbe, PrivateAskToolProbe, SessionIsolationEvidence, PRIVATE_ASK_TOOL_PROBE_ID,
};
use super::egress_proxy::EgressProxy;
use super::probe_program::{self, ProbeMarker};
use super::recovery::{
    finish_after_process, finish_before_spawn, leave_process_pending, leave_process_pending_state,
};
use super::session_evidence::SessionObservation;
use super::{PrivateAskFailure, SelectedAgentState};
use crate::managed_agents::discovery::bounded_command::{
    output_with_policy_and_spawn_hook, BoundedFailure, BoundedPolicy, OutputBudget,
};
use crate::managed_agents::recap_ownership::VerifiedStagingOwnership;
use crate::managed_agents::recap_state::OwnedRecapRun;
use sha2::{Digest, Sha256};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Wall-clock bound for one probe. Shorter than an answer's: the probe makes a
/// fixed number of local syscalls and never waits on a model.
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// Bytes retained from the probe's own streams.
const PROBE_STDOUT_LIMIT: u64 = 64 * 1024;
const PROBE_STDERR_LIMIT: u64 = 64 * 1024;

/// A host the proxy must refuse. It is a reserved test domain, so a refusal is
/// a policy fact rather than an accident of what happens to be unresolvable.
const FOREIGN_HOST: &str = "probe-foreign.invalid";

/// A listener this process owns, used as the direct-connect target.
///
/// Its accept count is the `direct_connections` the observation carries. A
/// contained child must not be able to reach it at all: it is the control that
/// distinguishes "the policy blocked everything" from "the proxy was never
/// tested", which a proxy record alone cannot.
struct ControlListener {
    address: String,
    accepted: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
}

impl ControlListener {
    fn start() -> Result<Self, PrivateAskFailure> {
        let listener =
            TcpListener::bind("127.0.0.1:0").map_err(|_| PrivateAskFailure::InvalidState)?;
        let address = listener
            .local_addr()
            .map_err(|_| PrivateAskFailure::InvalidState)?
            .to_string();
        listener
            .set_nonblocking(true)
            .map_err(|_| PrivateAskFailure::InvalidState)?;
        let accepted = Arc::new(AtomicU32::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let counter = Arc::clone(&accepted);
        let halt = Arc::clone(&stop);
        // One dedicated thread with a polling accept loop, bounded by `stop`.
        // It holds no handle on anything the relay funnel can reach, so the
        // thread-scoped no-publish attribution is unaffected.
        std::thread::spawn(move || {
            while !halt.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok(_) => {
                        counter.fetch_add(1, Ordering::AcqRel);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            address,
            accepted,
            stop,
        })
    }

    fn accepted(&self) -> u32 {
        self.accepted.load(Ordering::Acquire)
    }
}

impl Drop for ControlListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

/// Everything the probe needs that it cannot observe for itself.
pub(super) struct ProbeContext<'a> {
    pub(super) state: &'a SelectedAgentState,
    pub(super) ownership: &'a VerifiedStagingOwnership,
    /// The live employee session to bracket, when the selected agent has one.
    ///
    /// The bracketing happens HERE rather than in the caller because the
    /// lineage half of the evidence is the probe child's own reported parent,
    /// which only this module sees. A caller that supplied finished evidence
    /// would have to answer that question from its own PID, and a run that had
    /// been reparented would still look independent.
    pub(super) session: Option<SessionObservation>,
    pub(super) now: u64,
}

/// Run one probe and capture its trace, or refuse.
///
/// A refusal here means the probe could not be *performed*. A probe that was
/// performed and observed an escape is not a refusal: it returns a trace whose
/// dimensions are `Unverified`, so the viewer hears the specific refusal from
/// admission rather than a generic failure.
pub(super) fn capture_probe(
    context: ProbeContext<'_>,
) -> Result<PrivateAskProbe, PrivateAskFailure> {
    let staging_base = context
        .ownership
        .recap_base()
        .map_err(PrivateAskFailure::State)?;
    let run =
        OwnedRecapRun::create(&staging_base, context.now).map_err(PrivateAskFailure::State)?;

    // The run root must be a child of the staging base, which is what
    // `fresh_and_placed` structurally requires of the retained trace.
    let run_root = run.path().to_path_buf();
    capture_inside(&context, &staging_base, &run_root, run)
}

/// Run the probe, owning its run root through every exit.
///
/// The recovery discipline is the launch path's, not a single blind cleanup:
/// a failure before spawn removes the root, a failure after the process ran
/// closes it out through `finish_after_process`, and a run whose child PID
/// could not be recorded keeps its durable pending marker so startup recovery
/// can still reap it. A probe that dropped the root on a path where the process
/// boundary is unknown would leak exactly the process the boundary exists to
/// account for.
fn capture_inside(
    context: &ProbeContext<'_>,
    staging_base: &Path,
    run_root: &Path,
    mut run: OwnedRecapRun,
) -> Result<PrivateAskProbe, PrivateAskFailure> {
    macro_rules! before_spawn {
        ($expression:expr) => {
            match $expression {
                Ok(value) => value,
                Err(failure) => return Err(finish_before_spawn(run, failure)),
            }
        };
    }

    let state = context.state;
    let executable = &state.executable;

    // The sentinel lives outside the run root and is owned by this process. It
    // is what turns "the probe says the write failed" into "the bytes did not
    // change", and it is non-empty so a *read* that succeeds is distinguishable
    // from a read that returned nothing.
    let sentinel = staging_base.join(format!(".crew-probe-sentinel-{}", uuid::Uuid::new_v4()));
    before_spawn!(
        std::fs::write(&sentinel, b"crew-private-ask-probe-sentinel\n")
            .map_err(|_| PrivateAskFailure::InvalidState)
    );
    let sentinel_guard = SentinelGuard(sentinel.clone());
    let sentinel_before = before_spawn!(digest_file(&sentinel));
    let external_state_before =
        before_spawn!(external_state_digest(staging_base, run_root, &sentinel));

    // Read before anything is spawned: an "after" that has nothing to compare
    // with is not evidence of an unchanged session.
    let session_before = before_spawn!(context
        .session
        .as_ref()
        .map(SessionObservation::capture)
        .transpose());

    let control = before_spawn!(ControlListener::start());

    // The provider host is derived exactly as a launch derives it, so the probe
    // is bound to the same single destination a real answer would be.
    let provider_host = before_spawn!(super::provider::provider_host(
        &state.runtime_id,
        staged_profile_provider(context, run_root).as_deref(),
    ));
    let proxy = before_spawn!(EgressProxy::start(provider_host));

    let runtime_directory =
        before_spawn!(super::launch::runtime_directory(&executable.resolved_path));
    let extra_read_roots = super::launch::extra_read_roots(&executable.resolved_path);
    let containment_profile = before_spawn!(super::containment::private_ask_containment_profile(
        run_root,
        &runtime_directory,
        &extra_read_roots,
        proxy.port(),
    ));

    before_spawn!(super::launch::prepare_state_dirs(run_root));
    let program = before_spawn!(probe_program::write_probe_program(run_root));
    let lock_path = run_root.join("probe-descendant.lock");

    // The nonce is generated here, handed to the probe, and required back in
    // the marker. Without that round trip a retained marker from any earlier
    // run would be indistinguishable from this one's.
    let run_nonce = format!("probe-{}", uuid::Uuid::new_v4().simple());

    let mut command = std::process::Command::new("/usr/bin/sandbox-exec");
    command.arg("-p").arg(&containment_profile);
    command.arg(probe_program::PROBE_INTERPRETER).arg(&program);
    command
        .env_clear()
        .envs(super::launch::isolated_env(
            run_root,
            &executable.resolved_path,
        ))
        .current_dir(run_root)
        .stdin(std::process::Stdio::null());
    for (name, value) in proxy.child_env() {
        command.env(name, value);
    }
    command.env(probe_program::ENV_NONCE, &run_nonce);
    command.env(probe_program::ENV_SENTINEL, &sentinel);
    command.env(probe_program::ENV_CONTROL, &control.address);
    command.env(
        probe_program::ENV_PROXY,
        format!("127.0.0.1:{}", proxy.port()),
    );
    command.env(probe_program::ENV_PROVIDER, proxy.provider_host().as_str());
    command.env(probe_program::ENV_FOREIGN, FOREIGN_HOST);
    command.env(probe_program::ENV_LOCK, &lock_path);

    before_spawn!(run.mark_process_pending().map_err(PrivateAskFailure::State));
    let cancelled = AtomicBool::new(false);
    // The owned child PID is persisted before any output is consumed. Without
    // it a crash between spawn and completion leaves a pending root with no
    // process identity, which recovery can never reap — it would stay pending
    // forever. This is the launch path's contract and the probe owes the same.
    let mut pid_error = None;
    let output = output_with_policy_and_spawn_hook(
        command,
        BoundedPolicy {
            timeout: PROBE_TIMEOUT,
            budget: OutputBudget::PerStream {
                stdout: PROBE_STDOUT_LIMIT,
                stderr: PROBE_STDERR_LIMIT,
            },
        },
        &cancelled,
        |pid| {
            run.mark_process_started(pid).map_err(|failure| {
                pid_error = Some(failure);
                BoundedFailure::Cleanup
            })
        },
    );
    let output = match output {
        Ok(outcome) => outcome,
        Err(BoundedFailure::Cleanup) => {
            // Teardown was uncertain, or the PID was never recorded. Either way
            // the process boundary is unknown, so the durable pending marker
            // must survive for startup recovery instead of being removed here.
            if let Some(failure) = pid_error {
                return Err(leave_process_pending_state(run, failure));
            }
            return Err(leave_process_pending(run, BoundedFailure::Cleanup));
        }
        Err(failure) => {
            return Err(finish_after_process(
                run,
                PrivateAskFailure::Process(failure),
            ));
        }
    };

    macro_rules! after_process {
        ($expression:expr) => {
            match $expression {
                Ok(value) => value,
                Err(failure) => return Err(finish_after_process(run, failure)),
            }
        };
    }

    // Measured only after the bounded owner returned, so "surviving" means
    // survived teardown rather than "was alive during the run".
    let surviving_descendants = surviving_descendants(&lock_path);

    let marker = after_process!(
        probe_program::parse_marker(&output.output.stdout, &run_nonce)
            .ok_or(PrivateAskFailure::InvalidOutput)
    );

    let sentinel_after = after_process!(digest_file(&sentinel));
    let external_state_after =
        after_process!(external_state_digest(staging_base, run_root, &sentinel));
    let direct_connections = control.accepted();
    // Authentication is observed while the proxy is still serving, because that
    // is the gate `stage_private_ask_credential` enforces: no bearer credential
    // may be created for a run whose egress is not already bounded. Recording it
    // after the proxy was consumed would have meant re-expressing the gate here
    // instead of passing through it.
    let auth =
        super::credential::observe_auth_evidence(&state.runtime_id, run_root, &proxy, &cancelled);
    // Only now stop the proxy and take its record: after this point nothing may
    // reach the network on this attempt's behalf.
    // `with_observed_direct_connections` is how a probe supplies the count its
    // own listener measured; a production run has no such listener and
    // truthfully reports zero.
    let egress = proxy
        .observe(direct_connections)
        .with_observed_direct_connections(direct_connections);
    drop(sentinel_guard);

    let probe = PrivateAskProbe {
        runtime_id: state.runtime_id.clone(),
        executable: executable.clone(),
        effective_model: state.effective_model.clone(),
        profile: state.profile.clone(),
        persona: state.persona.clone(),
        acl_fingerprint: state.acl_fingerprint.clone(),
        session_generation: state.session_generation.clone(),
        auth,
        tool_probe: tool_probe_from(
            &marker,
            sentinel_before,
            sentinel_after,
            direct_connections,
            surviving_descendants,
        ),
        containment_profile,
        probe_run_root: run_root.to_path_buf(),
        egress,
        captured_at: context.now,
        run_nonce,
        staging_base: staging_base.to_path_buf(),
        external_state_before,
        external_state_after,
        session_isolation: after_process!(session_isolation(
            context.session.as_ref(),
            session_before,
            marker.parent_pid,
        )),
        probe_program_digest: probe_program::probe_program_digest(),
    };

    // Close the generation out before returning. A probe that returned its
    // trace while leaving a finished root behind would have the next probe
    // inherit it, and `fresh_and_placed` would then be describing a root this
    // run did not create.
    after_process!(run.mark_finished().map_err(PrivateAskFailure::State));
    run.cleanup().map_err(PrivateAskFailure::State)?;
    Ok(probe)
}

/// Pair the two session readings with the child's own reported lineage.
///
/// `None` when no session was named — a selection with no live session leaves
/// `independent_invocation` unverified, which is what refuses a busy agent.
/// A session that was readable before and is not now is an error rather than a
/// missing dimension: it changed under the run.
fn session_isolation(
    session: Option<&SessionObservation>,
    before: Option<super::session_evidence::SessionSnapshot>,
    child_parent_pid: u32,
) -> Result<Option<SessionIsolationEvidence>, PrivateAskFailure> {
    let (Some(session), Some(before)) = (session, before) else {
        return Ok(None);
    };
    let after = session.capture()?;
    SessionIsolationEvidence::observe(before, after, child_parent_pid, std::process::id()).map(Some)
}

/// Project the probe's attempts and this process's measurements onto the
/// retained tool-probe record.
///
/// A dimension is positive only when the attempt was made AND the measured
/// effect did not happen. An attempt that was never made proves nothing, which
/// is why `request_observed` is its own field rather than an assumption.
fn tool_probe_from(
    marker: &ProbeMarker,
    sentinel_before: String,
    sentinel_after: String,
    direct_connections: u32,
    surviving_descendants: u32,
) -> PrivateAskToolProbe {
    PrivateAskToolProbe {
        probe_id: PRIVATE_ASK_TOOL_PROBE_ID.to_owned(),
        tool_name: "write_file".to_owned(),
        // The attempt is always made by the shipped program; the flag exists
        // because the record must be able to say it was not.
        request_observed: true,
        // "Denied before effect" needs BOTH the probe's own failed syscall and
        // this process's own measurement. Either alone is weaker than the
        // claim: the first is a self-report, and an unchanged sentinel could
        // also hold if the write had landed somewhere else entirely. The other
        // attempts join it because this flag is what
        // `valid_tool_probe_evidence` reads as "the hostile attempt was
        // refused", and a run that escaped by any of these routes did not
        // demonstrate a refusal.
        denied_before_effect: marker.write_outside_denied
            && marker.direct_connect_denied
            && marker.dns_denied
            && marker.fork_denied
            && marker.foreign_connect_refused
            && marker.provider_connect_reached_proxy
            && sentinel_before == sentinel_after,
        sentinel_before,
        sentinel_after,
        read_outside_requested: true,
        read_outside_denied: marker.read_outside_denied,
        network_connections_observed: direct_connections,
        surviving_descendants,
    }
}

/// One if a descendant is still holding the lock, zero otherwise.
///
/// A missing lock file means the fork never produced a descendant, which is the
/// contained outcome. A lock that cannot be taken means something is still
/// holding it after the bounded owner completed — a real escape, counted rather
/// than assumed away.
fn surviving_descendants(lock_path: &Path) -> u32 {
    use rustix::fs::{flock, FlockOperation};

    let Ok(file) = std::fs::File::open(lock_path) else {
        return 0;
    };
    match flock(&file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => {
            let _ = flock(&file, FlockOperation::Unlock);
            0
        }
        Err(_) => 1,
    }
}

/// Remove the sentinel whatever happens to the probe.
struct SentinelGuard(std::path::PathBuf);

impl Drop for SentinelGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn digest_file(path: &Path) -> Result<String, PrivateAskFailure> {
    let bytes = std::fs::read(path).map_err(|_| PrivateAskFailure::InvalidState)?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}

/// Digest of the observable state outside the run root.
///
/// The run root itself is excluded: it is this attempt's own disposable tree
/// and is *expected* to change. The sentinel's bytes are included, so a write
/// that landed there moves this digest too.
fn external_state_digest(
    staging_base: &Path,
    run_root: &Path,
    sentinel: &Path,
) -> Result<String, PrivateAskFailure> {
    let mut entries: Vec<String> = Vec::new();
    let read = std::fs::read_dir(staging_base).map_err(|_| PrivateAskFailure::InvalidState)?;
    for entry in read {
        let entry = entry.map_err(|_| PrivateAskFailure::InvalidState)?;
        let path = entry.path();
        if path == run_root {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|_| PrivateAskFailure::InvalidState)?;
        entries.push(format!(
            "{}:{}:{}",
            entry.file_name().to_string_lossy(),
            u8::from(metadata.is_dir()),
            metadata.len()
        ));
    }
    entries.sort();
    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update(entry.as_bytes());
        hasher.update([0u8]);
    }
    hasher.update(digest_file(sentinel)?.as_bytes());
    Ok(hex::encode(hasher.finalize()))
}

/// The provider configured by an already-staged hermes profile, if any.
///
/// A probe stages no profile, so this is `None` today and the provider falls
/// back to the runtime's default. It is a function rather than a literal so the
/// staged-profile case has one place to grow into, and so the reason it is
/// `None` is written down rather than implied.
fn staged_profile_provider(_context: &ProbeContext<'_>, _run_root: &Path) -> Option<String> {
    None
}

#[cfg(test)]
#[path = "probe_run_tests.rs"]
mod tests;
