//! Session isolation: a private Ask runs beside a genuinely busy employee
//! session without touching anything that session owns.
//!
//! The busy session here is a real process holding a real ledger file. The
//! private Ask runs through the production launch path while it is still
//! working, and the assertions are taken from the ledger bytes, the observer
//! sequence and the live PID — not from the adapter's own report.

#![cfg(all(unix, target_os = "macos"))]

use super::capability::{
    PrivateAskAuthEvidence, PrivateAskProbe, PrivateAskToolProbe, SessionIsolationEvidence,
    PRIVATE_ASK_TOOL_PROBE_ID,
};
use super::session_evidence::SessionSnapshot;
use super::tests::{canonical_tempdir, executable, owned_receipt, request, state, FIXTURE_PERSONA};
use super::*;
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

/// A stand-in employee session: a live process that keeps appending to its own
/// ledger for a few seconds, exactly as a long agent turn would.
struct BusyEmployeeSession {
    child: Child,
    ledger: PathBuf,
    observer_sequence: PathBuf,
}

impl BusyEmployeeSession {
    fn start(directory: &Path) -> Self {
        let ledger = directory.join("employee-ledger.json");
        let observer_sequence = directory.join("employee-observer-sequence");
        std::fs::write(&ledger, b"{\"turn\":\"in-progress\",\"messages\":3}").expect("ledger");
        std::fs::write(&observer_sequence, b"42").expect("observer sequence");
        let child = std::process::Command::new("/usr/bin/perl")
            .arg("-e")
            .arg("$| = 1; print \"busy\\n\"; sleep 30;")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("busy employee session");
        let mut session = Self {
            child,
            ledger,
            observer_sequence,
        };
        session.wait_until_working();
        session
    }

    /// Block only until the session has actually started working, with a
    /// deadline: a test must never wait on a process that failed to start.
    fn wait_until_working(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if self.is_alive() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the busy employee session never started");
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Ask the child itself, not the PID namespace: a `kill(pid, 0)` probe can
    /// be answered by a recycled PID, and this process owns the handle anyway.
    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn ledger_digest(&self) -> String {
        hex::encode(Sha256::digest(std::fs::read(&self.ledger).expect("ledger")))
    }

    fn observer_sequence(&self) -> u64 {
        std::fs::read_to_string(&self.observer_sequence)
            .expect("observer sequence")
            .trim()
            .parse()
            .expect("observer sequence is numeric")
    }
}

impl Drop for BusyEmployeeSession {
    fn drop(&mut self) {
        // Stop only what this test started.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A private Ask runtime that reports what it can actually see: its own parent,
/// its `HOME`, and whether the employee's ledger was reachable.
fn reporting_runtime(directory: &Path, employee_ledger: &Path) -> PathBuf {
    let script = format!(
        "#!/usr/bin/perl\n\
         local $/;\n\
         my $prompt = <STDIN>;\n\
         my $home = $ENV{{HOME}} // 'unset';\n\
         my $reached = open(my $handle, '>>', '{ledger}') ? 'reached' : 'denied';\n\
         close $handle if $handle;\n\
         my $trace = 'parent=' . getppid() . \" home=$home employee-ledger=$reached\";\n\
         $trace =~ s/\\\\/\\\\\\\\/g;\n\
         $trace =~ s/\"/\\\\\"/g;\n\
         print '{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"'\n\
             . $trace\n\
             . '\",\"modelUsage\":{{\"claude-fable-5-1\":{{}}}}}}';\n",
        ledger = employee_ledger.display(),
    );
    let path = directory.join("reporting-runtime");
    std::fs::write(&path, script).expect("runtime fixture");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).expect("permissions");
    path
}

/// Mirror of what `from_probe` resolves, so the rebuilt policy compares equal.
fn runtime_directory_of(executable: &Path) -> PathBuf {
    executable
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn digest_of(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

/// A private Ask running beside a busy employee
/// session leaves that session's ledger bytes, observer sequence and process
/// untouched, is parented to this process rather than to the session, and never
/// sees the employee's home. Removing the `HOME` entry from `isolated_env`
/// fails the home assertion; removing the `sandbox-exec` wrapper from
/// `PrivateAskLaunchPlan::command` fails the ledger-denial assertion.
#[test]
fn a_private_ask_beside_a_busy_employee_session_changes_nothing_that_session_owns() {
    let directory = canonical_tempdir();
    let mut session = BusyEmployeeSession::start(directory.path());
    let ledger_before = session.ledger_digest();
    let observer_before = session.observer_sequence();
    let acp_pid_before = session.pid();
    let snapshot_before = SessionSnapshot::capture(
        &session.ledger,
        &session.observer_sequence,
        Some(acp_pid_before),
    )
    .expect("observe the busy session before the Ask");

    let runtime = reporting_runtime(directory.path(), &session.ledger);
    let mut selected = state(&runtime, "claude", "claude-fable-5-1", None);
    selected.executable = executable(&runtime);
    selected.lifecycle = AgentLifecycle::Busy;
    let capability = PrivateAskCapability::verified_for_fixture(&selected);
    let admission =
        admit_private_ask(request(), selected.clone(), capability).expect("busy admission");
    let ownership = owned_receipt(&directory);
    let base = ownership.recap_base().expect("recap base");
    let attempt = PrivateAskAttempt::create(admission, ownership, 1).expect("attempt");
    let response = attempt.run().expect("private Ask beside a busy session");

    // The employee session is still exactly where it was.
    assert!(session.is_alive(), "the busy session must not be disturbed");
    assert_eq!(session.pid(), acp_pid_before, "no session was restarted");
    assert_eq!(session.ledger_digest(), ledger_before, "ledger was mutated");
    assert_eq!(session.observer_sequence(), observer_before);

    // The private child was owned by this process, not adopted by the session,
    // and never saw the employee's home or ledger.
    assert!(
        response
            .markdown
            .contains(&format!("parent={}", std::process::id())),
        "trace: {}",
        response.markdown
    );
    assert!(
        response.markdown.contains("employee-ledger=denied"),
        "trace: {}",
        response.markdown
    );
    let home = response
        .markdown
        .split("home=")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .expect("home in trace")
        .to_owned();
    assert!(
        home.starts_with(base.join("recap-runs").to_str().unwrap()) && home.ends_with("/home"),
        "the child's home must be its own run root, saw {home}"
    );

    // The real observation above is what certifies independence; nothing else
    // in this module can set that dimension.
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
            request_observed: true,
            denied_before_effect: response.markdown.contains("employee-ledger=denied"),
            read_outside_requested: true,
            read_outside_denied: response.markdown.contains("employee-ledger=denied"),
            sentinel_before: ledger_before.clone(),
            sentinel_after: session.ledger_digest(),
            network_connections_observed: 0,
            surviving_descendants: 0,
        },
        containment_profile: containment::private_ask_containment_profile(
            &run_root,
            &runtime_directory_of(&selected.executable.resolved_path),
        )
        .unwrap(),
        probe_run_root: run_root,
        external_state_before: digest_of("checkout"),
        external_state_after: digest_of("checkout"),
        // Minted by the production producer from the session's own bytes, not
        // assembled here: `SessionIsolationEvidence`'s fields are private to
        // `session_evidence`, so this test cannot describe a session it did
        // not actually observe.
        session_isolation: Some(
            SessionIsolationEvidence::observe(
                snapshot_before,
                SessionSnapshot::capture(
                    &session.ledger,
                    &session.observer_sequence,
                    Some(session.pid()),
                )
                .expect("observe the busy session after the Ask"),
                std::process::id(),
                std::process::id(),
            )
            .expect("session lineage"),
        ),
    };
    let capability = PrivateAskCapability::from_probe(&selected, probe).expect("projection");
    assert_eq!(capability.independent_invocation, ProofStatus::Verified);
    // The busy-session fence is satisfied by a real observation: the request no
    // longer fails as `AgentBusy`. It stops at the egress bound instead, which
    // nothing can yet prove.
    assert_eq!(
        admit_private_ask(request(), selected, capability).unwrap_err(),
        PrivateAskFailure::EgressBoundUnverified
    );
}

/// Without that observation a busy agent is refused, and the refusal names the
/// busy session rather than a generic unverified capability. Removing the
/// `AgentLifecycle::Busy` fence from `admit_private_ask` fails this.
#[test]
fn a_busy_agent_without_an_independence_observation_is_refused_as_busy() {
    let directory = canonical_tempdir();
    let path = directory.path().join("runtime");
    let mut selected = state(&path, "claude", "claude-fable-5-1", None);
    selected.lifecycle = AgentLifecycle::Busy;
    let mut capability = PrivateAskCapability::verified_for_fixture(&selected);
    capability.independent_invocation = ProofStatus::Unverified;
    assert_eq!(
        admit_private_ask(request(), selected.clone(), capability).unwrap_err(),
        PrivateAskFailure::AgentBusy
    );

    // A session snapshot whose PID moved is not an independent invocation.
    let restarted = SessionIsolationEvidence::from_parts(
        digest_of("ledger"),
        digest_of("ledger"),
        1,
        1,
        Some(11),
        Some(12),
        std::process::id(),
        std::process::id(),
    );
    let run_root = directory.path().join("probe-root");
    std::fs::create_dir(&run_root).unwrap();
    let probe_state = state(&path, "claude", "claude-fable-5-1", None);
    let build = |isolation: SessionIsolationEvidence, root: PathBuf| PrivateAskProbe {
        runtime_id: "claude".into(),
        executable: probe_state.executable.clone(),
        effective_model: probe_state.effective_model.clone(),
        profile: None,
        persona: FIXTURE_PERSONA.into(),
        acl_fingerprint: probe_state.acl_fingerprint.clone(),
        session_generation: probe_state.session_generation.clone(),
        auth: PrivateAskAuthEvidence {
            service: "buzz-desktop-demo.staging-test".into(),
            reference: "keychain-reference".into(),
            auth_available: true,
        },
        tool_probe: PrivateAskToolProbe {
            probe_id: PRIVATE_ASK_TOOL_PROBE_ID.into(),
            tool_name: "write_file".into(),
            request_observed: true,
            denied_before_effect: true,
            read_outside_requested: true,
            read_outside_denied: true,
            sentinel_before: digest_of("sentinel"),
            sentinel_after: digest_of("sentinel"),
            network_connections_observed: 0,
            surviving_descendants: 0,
        },
        containment_profile: containment::private_ask_containment_profile(
            &root,
            &runtime_directory_of(&probe_state.executable.resolved_path),
        )
        .unwrap_or_default(),
        probe_run_root: root,
        external_state_before: digest_of("checkout"),
        external_state_after: digest_of("checkout"),
        session_isolation: Some(isolation),
    };
    let capability =
        PrivateAskCapability::from_probe(&probe_state, build(restarted.clone(), run_root.clone()))
            .expect("projection");
    assert_eq!(capability.independent_invocation, ProofStatus::Unverified);

    // So is a child adopted by something other than the desktop process.
    let adopted = SessionIsolationEvidence::from_parts(
        digest_of("ledger"),
        digest_of("ledger"),
        1,
        1,
        Some(11),
        Some(11),
        std::process::id() + 1,
        std::process::id(),
    );
    let capability = PrivateAskCapability::from_probe(&probe_state, build(adopted, run_root))
        .expect("projection");
    assert_eq!(capability.independent_invocation, ProofStatus::Unverified);
}
