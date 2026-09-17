//! Real-process proof that a hostile private Ask runtime is denied its effects.
//!
//! These tests run an actual process through the production launch path. The
//! control run establishes that the fixture really can write outside its run
//! root, open a socket and fork a setsid descendant; the contained run asserts
//! the inverse through exactly the same plan. Deleting the `sandbox-exec`
//! wrapper from `PrivateAskLaunchPlan::command` makes the contained run behave
//! like the control and fails these tests.

#![cfg(all(unix, target_os = "macos"))]

use super::capability::{
    PrivateAskAuthEvidence, PrivateAskProbe, PrivateAskToolProbe, PRIVATE_ASK_TOOL_PROBE_ID,
};
use super::tests::{
    canonical_tempdir, captured_now, executable, owned_receipt, request, state, FIXTURE_PERSONA,
};
use super::*;
use sha2::{Digest, Sha256};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

const FIXTURE_SOURCE: &str = include_str!("../../../test-fixtures/private-ask-hostile-runtime.pl");

struct HostileFixture {
    executable: PathBuf,
    sentinel: PathBuf,
    /// A file outside the run root holding bytes the run must never see. It
    /// lives in its own directory, NOT beside the runtime executable: the
    /// policy allows the runtime's own directory so the binary can load
    /// itself, and a secret parked there would be legitimately readable.
    readable: PathBuf,
    /// Owns the lifetime of that directory.
    _secrets: tempfile::TempDir,
    descendant_marker: PathBuf,
    descendant_pid_file: PathBuf,
    listener_port: u16,
    accepted: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    listener_thread: Option<std::thread::JoinHandle<()>>,
}

impl HostileFixture {
    fn create(directory: &Path, name: &str) -> Self {
        let sentinel = directory.join("sentinel-outside-run-root");
        std::fs::write(&sentinel, b"untouched").expect("sentinel");
        let secrets = canonical_tempdir();
        let readable = secrets.path().join("employee-secret-outside-run-root");
        std::fs::write(&readable, b"employee-secret").expect("readable");
        let descendant_marker = directory.join("descendant-marker");
        let descendant_pid_file = directory.join("descendant-pid");

        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let listener_port = listener.local_addr().expect("listener addr").port();
        listener
            .set_nonblocking(true)
            .expect("non-blocking listener");
        let accepted = Arc::new(AtomicU32::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_accepted = Arc::clone(&accepted);
        let thread_stop = Arc::clone(&stop);
        // A bounded accept loop: it never blocks the test and always exits.
        let listener_thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok(_) => {
                        thread_accepted.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        let script = FIXTURE_SOURCE
            .replace("__SENTINEL__", sentinel.to_str().expect("sentinel path"))
            .replace("__PORT__", &listener_port.to_string())
            .replace(
                "__MARKER__",
                descendant_marker.to_str().expect("marker path"),
            )
            .replace(
                "__PID_FILE__",
                descendant_pid_file.to_str().expect("pid path"),
            )
            .replace("__READABLE__", readable.to_str().expect("readable path"));
        let executable = directory.join(name);
        std::fs::write(&executable, script).expect("fixture");
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
            .expect("fixture permissions");

        Self {
            executable,
            sentinel,
            readable,
            _secrets: secrets,
            descendant_marker,
            descendant_pid_file,
            listener_port,
            accepted,
            stop,
            listener_thread: None.or(Some(listener_thread)),
        }
    }

    fn sentinel_digest(&self) -> String {
        let bytes = std::fs::read(&self.sentinel).expect("sentinel readable");
        hex::encode(Sha256::digest(&bytes))
    }

    /// Accept count observed so far, after giving the loop a moment to catch a
    /// connection that is already in flight.
    fn accepted_connections(&self) -> u32 {
        std::thread::sleep(Duration::from_millis(200));
        self.accepted.load(Ordering::Relaxed)
    }

    fn descendant_pid(&self) -> Option<u32> {
        std::fs::read_to_string(&self.descendant_pid_file)
            .ok()
            .and_then(|value| value.trim().parse().ok())
    }

    /// A descendant that recorded its own PID is a descendant that was created.
    /// Under the production policy the fork is refused, so no PID file exists at
    /// all; in the control run the fixture waits for its own child before
    /// exiting, so the file is present and the process is already reaped. A
    /// `kill(pid, 0)` liveness probe is deliberately not used: it can be
    /// answered by a recycled PID and would turn a real escape into a pass.
    fn recorded_descendants(&self) -> u32 {
        u32::from(self.descendant_pid_file.exists())
    }
}

impl Drop for HostileFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Unblock the accept loop deterministically instead of waiting for its
        // poll interval.
        let _ = TcpStream::connect(("127.0.0.1", self.listener_port));
        if let Some(thread) = self.listener_thread.take() {
            let _ = thread.join();
        }
    }
}

/// The directory `from_probe` will rebuild the policy from. A test that names a
/// different one would compare two unequal policies and fail for the wrong
/// reason.
fn probe_runtime_directory(executable: &Path) -> PathBuf {
    executable
        .parent()
        .expect("runtime parent")
        .canonicalize()
        .expect("canonical runtime directory")
}

fn digest_of(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn contained_attempt(
    fixture_dir: &tempfile::TempDir,
    fixture: &HostileFixture,
) -> (Result<PrivateAskResponse, PrivateAskFailure>, PathBuf) {
    let mut selected = state(&fixture.executable, "claude", "claude-fable-5-1", None);
    selected.executable = executable(&fixture.executable);
    let capability = PrivateAskCapability::verified_for_fixture(&selected);
    let admission = admit_private_ask(request(), selected, capability).expect("admission");
    let ownership = owned_receipt(fixture_dir);
    let base = ownership.recap_base().expect("recap base");
    let attempt = PrivateAskAttempt::create(admission, ownership, 1).expect("attempt");
    (attempt.run(), base)
}

/// The control half: the fixture really is hostile, so the contained half is
/// not passing for want of an attempt.
#[test]
fn the_hostile_fixture_reaches_every_effect_when_it_is_not_contained() {
    let directory = canonical_tempdir();
    let fixture = HostileFixture::create(directory.path(), "uncontained-runtime");
    let before = fixture.sentinel_digest();

    let mut command = std::process::Command::new(&fixture.executable);
    command.env_clear().current_dir(directory.path());
    let cancelled = AtomicBool::new(false);
    let outcome = super::super::discovery::bounded_command::output_with_policy(
        command,
        BoundedPolicy {
            timeout: Duration::from_secs(20),
            budget: OutputBudget::PerStream {
                stdout: 64 * 1024,
                stderr: 64 * 1024,
            },
        },
        &cancelled,
    )
    .expect("uncontained control run");
    let trace = String::from_utf8_lossy(&outcome.output.stdout).into_owned();

    assert!(outcome.output.status.success(), "control trace: {trace}");
    assert!(trace.contains("effect:write-outside"), "trace: {trace}");
    assert!(
        trace.contains("effect:read-outside"),
        "an uncontained fixture must read the file outside its run root: {trace}"
    );
    assert!(trace.contains("effect:connect"), "trace: {trace}");
    assert!(
        trace.contains("effect:dns"),
        "an uncontained fixture must be able to resolve a name: {trace}"
    );
    assert!(trace.contains("effect:fork"), "trace: {trace}");
    assert_ne!(
        fixture.sentinel_digest(),
        before,
        "an uncontained fixture must change the sentinel"
    );
    assert_eq!(
        std::fs::read(&fixture.descendant_marker).unwrap(),
        b"escaped"
    );
    assert!(fixture.accepted_connections() >= 1);
    assert_eq!(
        fixture.recorded_descendants(),
        1,
        "an uncontained fixture must create a setsid descendant"
    );
    assert!(
        fixture.descendant_pid().is_some(),
        "the control descendant must record its own PID"
    );
}

/// The proof: the same fixture, run through the production launch path, reaches
/// none of its effects, leaves no descendant, and — because a hostile trace is
/// not a certification — still cannot have its answer admitted.
#[test]
fn a_hostile_runtime_is_denied_every_effect_through_the_production_launch_path() {
    let directory = canonical_tempdir();
    let fixture = HostileFixture::create(directory.path(), "contained-runtime");
    let sentinel_before = fixture.sentinel_digest();

    let (result, base) = contained_attempt(&directory, &fixture);
    let trace = result.expect("the contained runtime still produces bounded output");
    let sentinel_after = fixture.sentinel_digest();

    // Every hostile attempt was made and every one of them was refused.
    assert!(trace.markdown.contains("attempt:write-outside"));
    assert!(trace.markdown.contains("attempt:connect"));
    assert!(trace.markdown.contains("attempt:fork"));
    assert!(
        trace.markdown.contains("denied:write-outside"),
        "trace: {}",
        trace.markdown
    );
    assert!(
        trace.markdown.contains("denied:connect"),
        "trace: {}",
        trace.markdown
    );
    assert!(
        trace.markdown.contains("denied:fork"),
        "trace: {}",
        trace.markdown
    );
    assert!(
        trace.markdown.contains("denied:read-outside"),
        "a contained run must not read a file outside its run root: {}",
        trace.markdown
    );
    // The proxy is the only egress, and it refused the one destination that was
    // not the provider. Removing the host check in `egress_proxy::handle_client`
    // turns this into `effect:proxy-connect`.
    assert!(
        trace.markdown.contains("denied:proxy-connect"),
        "the proxy must refuse a destination that is not the provider: {}",
        trace.markdown
    );
    // No resolver is reachable. Restoring the mDNSResponder allowance to
    // `private_ask_containment_profile` turns this into `effect:dns`.
    assert!(
        trace.markdown.contains("denied:dns"),
        "a contained run must not resolve a name: {}",
        trace.markdown
    );
    assert!(!trace.markdown.contains("effect:"));

    // The refusal is in the proxy's own record, not only in the child's report.
    // A record with an empty refusal list cannot certify egress at all.
    assert!(
        trace.egress.accepted().is_empty(),
        "nothing may have been accepted: {:?}",
        trace.egress.accepted()
    );
    assert!(
        trace
            .egress
            .refused()
            .iter()
            .any(|refused| refused.reason() == super::egress_proxy::RefusalReason::ForeignHost),
        "the proxy must have recorded the foreign CONNECT: {:?}",
        trace.egress.refused()
    );

    assert_eq!(
        sentinel_after, sentinel_before,
        "sentinel must be unchanged"
    );
    assert!(!fixture.descendant_marker.exists());
    assert_eq!(fixture.accepted_connections(), 0);
    assert_eq!(
        fixture.recorded_descendants(),
        0,
        "no descendant may be created under the production policy"
    );
    // The disposable generation is gone; nothing outlives the attempt.
    assert!(!base
        .join("recap-runs")
        .read_dir()
        .unwrap()
        .any(|entry| entry.is_ok()));

    // The observation above is evidence, not permission. Projected onto the
    // selection it certifies containment, tool isolation and side-effect
    // freedom — and nothing else, because no busy-session snapshot was taken.
    let selected = state(&fixture.executable, "claude", "claude-fable-5-1", None);
    let mut selected = selected;
    selected.executable = executable(&fixture.executable);
    let run_root = directory.path().join("probe-root");
    std::fs::create_dir(&run_root).unwrap();
    let probe = PrivateAskProbe {
        runtime_id: "claude".into(),
        executable: selected.executable.clone(),
        effective_model: selected.effective_model.clone(),
        profile: None,
        persona: FIXTURE_PERSONA.into(),
        acl_fingerprint: selected.acl_fingerprint.clone(),
        session_generation: selected.session_generation.clone(),
        auth: PrivateAskAuthEvidence {
            service: "buzz-desktop-demo.staging-test".into(),
            reference: "keychain-reference".into(),
            auth_available: true,
        },
        tool_probe: PrivateAskToolProbe {
            probe_id: PRIVATE_ASK_TOOL_PROBE_ID.into(),
            tool_name: "write_file".into(),
            request_observed: trace.markdown.contains("attempt:write-outside"),
            denied_before_effect: !trace.markdown.contains("effect:"),
            read_outside_requested: trace.markdown.contains("attempt:read-outside"),
            read_outside_denied: !trace.markdown.contains("effect:read-outside"),
            sentinel_before,
            sentinel_after,
            network_connections_observed: fixture.accepted_connections(),
            surviving_descendants: fixture.recorded_descendants(),
        },
        containment_profile: containment::private_ask_containment_profile(
            &run_root,
            &probe_runtime_directory(&fixture.executable),
            &super::launch::extra_read_roots(&fixture.executable),
            // The port this run's own proxy actually bound. Rebuilding the
            // policy from anything else would compare two different policies.
            trace.egress.proxy_port(),
        )
        .unwrap(),
        staging_base: run_root
            .parent()
            .expect("probe root has a parent")
            .to_path_buf(),
        // The real record from the run above, carrying the fixture listener's
        // own count of connections that bypassed the proxy.
        egress: trace
            .egress
            .clone()
            .with_observed_direct_connections(fixture.accepted_connections()),
        captured_at: captured_now(),
        run_nonce: "fixture-run-nonce".into(),
        probe_run_root: run_root,
        external_state_before: digest_of("checkout-unchanged"),
        external_state_after: digest_of("checkout-unchanged"),
        session_isolation: None,
    };
    let capability = PrivateAskCapability::from_probe(&selected, probe).expect("probe projection");
    assert_eq!(capability.tool_isolation, ProofStatus::Verified);
    assert_eq!(capability.process_containment, ProofStatus::Verified);
    assert_eq!(capability.side_effect_free, ProofStatus::Verified);
    assert_eq!(capability.independent_invocation, ProofStatus::Unverified);
    // A hostile run never reached the provider, so its own proxy record cannot
    // bound egress either: an accepted-empty record is exactly the "blocked
    // everything" case that must not read as "correctly scoped".
    assert_eq!(capability.egress_bounded, ProofStatus::Unverified);
    assert_eq!(
        admit_private_ask(request(), selected, capability).unwrap_err(),
        PrivateAskFailure::IndependentInvocationUnverified,
        "a hostile-tool denial alone must not admit a private Ask"
    );
}

/// The executable actually confined is the runtime's real path, and the policy
/// is the effect-denying one — not the recap no-fork profile, which permits
/// both writes and network egress. Weakening the profile fails this.
#[test]
fn the_launch_command_applies_the_effect_denying_policy_to_the_runtime() {
    let directory = canonical_tempdir();
    let fixture = HostileFixture::create(directory.path(), "plan-runtime");
    let mut selected = state(&fixture.executable, "claude", "claude-fable-5-1", None);
    selected.executable = executable(&fixture.executable);
    let capability = PrivateAskCapability::verified_for_fixture(&selected);
    let admission = admit_private_ask(request(), selected, capability).expect("admission");
    let ownership = owned_receipt(&directory);
    let base = ownership.recap_base().expect("base");
    let run = super::super::recap_state::OwnedRecapRun::create(&base, 1).expect("run");
    let proxy = super::tests::fixture_proxy();
    let plan = PrivateAskLaunchPlan::for_admission(&admission, &run, &proxy).expect("plan");
    let command = plan.command();
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    assert_eq!(
        command.get_program(),
        std::ffi::OsStr::new("/usr/bin/sandbox-exec")
    );
    assert_eq!(args.first().map(String::as_str), Some("-p"));
    let profile = args.get(1).expect("policy text");
    assert!(profile.contains("(deny file-write*)"));
    assert!(profile.contains(&format!(
        "(allow file-write* (subpath \"{}\"))",
        run.path().display()
    )));
    assert!(profile.contains("(deny network-outbound)"));
    assert!(profile.contains("(deny process-fork)"));
    assert_ne!(profile, "(version 1)(allow default)(deny process-fork)");
    assert_eq!(
        args.get(2).map(String::as_str),
        fixture.executable.to_str(),
        "the runtime itself must be the wrapped program"
    );

    let mut run = run;
    run.mark_finished().unwrap();
    run.cleanup().unwrap();
}
