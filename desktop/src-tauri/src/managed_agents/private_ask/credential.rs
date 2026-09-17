//! Short-lived provider credential handed to one contained private Ask child.
//!
//! The shape is `wiki_runtime_auth`'s: the desktop process reads the existing
//! subscription credential from the platform secret store, and the child gets
//! it through its environment only. Nothing is written to the run root, nothing
//! reaches argv, and no value here is ever formatted into an error.
//!
//! The one addition is a gate. A token is a bearer credential, so it must not
//! be given to a process that could carry it anywhere: staging is refused
//! unless the attempt's own loopback proxy is already serving, which is the
//! only egress the Seatbelt policy leaves open. "Authenticated" and "bounded"
//! are therefore not independent states — the first cannot exist without the
//! second.

use super::egress_proxy::EgressProxy;
use super::PrivateAskFailure;
use crate::managed_agents::wiki_runtime::WikiRuntimeFailure;
use std::ffi::OsString;
use std::sync::atomic::AtomicBool;

/// Maximum credential length accepted from the secret store.
const CREDENTIAL_LIMIT: usize = 32 * 1024;

/// One environment entry carrying a bearer credential.
///
/// `Debug` is implemented by hand and prints only the variable name. A derived
/// `Debug` would put the token into any error, panic message or trace that ever
/// formats a value containing one, which is precisely the leak the tests assert
/// against.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct StagedCredential {
    name: &'static str,
    value: String,
}

impl StagedCredential {
    pub(super) fn name(&self) -> &'static str {
        self.name
    }

    pub(super) fn env_entry(&self) -> (OsString, OsString) {
        (OsString::from(self.name), OsString::from(&self.value))
    }
}

impl std::fmt::Debug for StagedCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StagedCredential")
            .field("name", &self.name)
            .field("value", &"<redacted>")
            .finish()
    }
}

/// Read and validate the provider credential for one runtime.
///
/// `Ok(None)` means the runtime owns its own authentication and needs no
/// handoff — that is hermes, whose staged profile carries its provider
/// configuration. It is deliberately distinct from an error: a runtime that
/// *should* have produced a token and did not must refuse, never launch
/// unauthenticated and report a model failure.
///
/// `state_dir` is the attempt's own run root. Claude needs no file staged, so
/// nothing is written there on this path; it is passed because the shared
/// reader's contract takes it, and passing the run root keeps any future file
/// handoff inside the boundary rather than outside it.
pub(super) fn stage_private_ask_credential(
    runtime_id: &str,
    state_dir: &std::path::Path,
    proxy: &EgressProxy,
    cancelled: &AtomicBool,
) -> Result<Option<StagedCredential>, PrivateAskFailure> {
    // The gate. Without a live proxy the child's only egress is unknown, so no
    // bearer credential may be created for it.
    if !proxy.is_listening() {
        return Err(PrivateAskFailure::EgressBoundUnverified);
    }
    match runtime_id {
        "hermes" => Ok(None),
        "claude" => {
            let token = crate::managed_agents::wiki_runtime_auth::stage_runtime_auth(
                "claude", state_dir, cancelled,
            )
            .map_err(map_auth_failure)?
            .ok_or(PrivateAskFailure::AuthenticationUnverified)?;
            Ok(Some(claude_credential(&token)?))
        }
        _ => Err(PrivateAskFailure::MissingRuntime),
    }
}

/// Build the Claude credential entry from a raw token, or refuse it.
///
/// Kept as the production seam so a test can drive it with a canary secret and
/// prove where that secret does and does not end up.
pub(super) fn claude_credential(token: &str) -> Result<StagedCredential, PrivateAskFailure> {
    if token.is_empty()
        || token.len() > CREDENTIAL_LIMIT
        || token != token.trim()
        || token.chars().any(char::is_control)
    {
        return Err(PrivateAskFailure::AuthenticationUnverified);
    }
    Ok(StagedCredential {
        // The same variable `wiki_runtime` uses. A private Ask must not invent
        // a second authentication contract for the same binary.
        name: "CLAUDE_CODE_OAUTH_TOKEN",
        value: token.to_owned(),
    })
}

/// Every credential failure collapses to one refusal.
///
/// The distinctions `wiki_runtime_auth` draws — unavailable, expired, invalid,
/// over limit — are useful to a setup flow and useless to a viewer, and each
/// one is a small oracle about the machine's secret store. The run refuses; the
/// developer reads the setup surface.
fn map_auth_failure(failure: WikiRuntimeFailure) -> PrivateAskFailure {
    let _ = failure;
    PrivateAskFailure::AuthenticationUnverified
}

/// Observe whether this runtime can actually authenticate, as its own evidence.
///
/// This is deliberately a separate dimension from containment. An earlier round
/// folded it into `PrivateAskCapability::from_probe`'s containment verdict and
/// that was reverted: it made a broken sandbox report itself as an
/// authentication failure, which sends a developer to the wrong problem.
///
/// The observation is the *outcome of the real staging path*, not a flag and
/// not the presence of a file: the same `stage_private_ask_credential` a launch
/// uses is driven here, subject to the same proxy gate, and its result is what
/// is recorded. `Ok(None)` — hermes, which owns its authentication inside its
/// staged profile — is availability, not absence; it is deliberately distinct
/// from a refusal.
///
/// Nothing derived from the secret reaches the evidence. `reference` names the
/// binding (runtime plus the environment entry it is handed through) so two
/// different authentication contracts are distinguishable, and carries no part
/// of the token.
#[cfg(not(test))]
pub(super) fn observe_auth_evidence(
    runtime_id: &str,
    state_dir: &std::path::Path,
    proxy: &EgressProxy,
    cancelled: &AtomicBool,
) -> super::capability::PrivateAskAuthEvidence {
    evidence_from(
        runtime_id,
        stage_private_ask_credential(runtime_id, state_dir, proxy, cancelled),
    )
}

/// The same observation with the platform secret store replaced by a canary.
///
/// A unit test must never read the developer's keychain. The real reader shells
/// out to `/usr/bin/security find-generic-password`, so without this seam the
/// evidence would depend on whether the machine running the suite happens to
/// have a Claude token — `Verified` on a developer's Mac and `Unverified` in
/// CI, for the same code. `PrivateAskAttempt::stage_credential` already splits
/// this way for exactly that reason, and this mirrors it.
///
/// What is NOT replaced is the gate: the canary is still refused unless the
/// attempt's proxy is serving, so both directions are observable and
/// deterministic. [`super::credential::tests`] drives the real
/// `stage_private_ask_credential` against that same gate.
#[cfg(test)]
pub(super) fn observe_auth_evidence(
    runtime_id: &str,
    _state_dir: &std::path::Path,
    proxy: &EgressProxy,
    _cancelled: &AtomicBool,
) -> super::capability::PrivateAskAuthEvidence {
    let staged = if !proxy.is_listening() {
        Err(PrivateAskFailure::EgressBoundUnverified)
    } else if test_secret_store_is_empty() {
        Err(PrivateAskFailure::AuthenticationUnverified)
    } else {
        match runtime_id {
            "hermes" => Ok(None),
            "claude" => {
                claude_credential(super::PrivateAskAttempt::TEST_CREDENTIAL_CANARY).map(Some)
            }
            _ => Err(PrivateAskFailure::MissingRuntime),
        }
    };
    evidence_from(runtime_id, staged)
}

/// Whether the stand-in secret store holds nothing for this observation.
///
/// It is THREAD-local, not process-global. `observe_auth_evidence` runs
/// synchronously on the thread that asked for the probe, so a thread-local is
/// exactly as reachable as the previous global — and it cannot leak into
/// another test running in parallel, which is what a process-global flag did:
/// a test that wanted the "no credential" direction made every concurrent
/// probe on the machine observe an empty store too.
#[cfg(test)]
thread_local! {
    static TEST_SECRET_STORE_EMPTY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(super) fn test_secret_store_is_empty() -> bool {
    TEST_SECRET_STORE_EMPTY.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(super) fn set_test_secret_store_empty(empty: bool) {
    TEST_SECRET_STORE_EMPTY.with(|slot| slot.set(empty));
}

/// Turn one staging outcome into evidence. Shared by both observers so the
/// test seam cannot describe availability differently from production.
fn evidence_from(
    runtime_id: &str,
    staged: Result<Option<StagedCredential>, PrivateAskFailure>,
) -> super::capability::PrivateAskAuthEvidence {
    let service = runtime_id.to_owned();
    match staged {
        Ok(Some(credential)) => super::capability::PrivateAskAuthEvidence::observed(
            service.clone(),
            format!("{service}:{}", credential.name()),
            true,
        ),
        Ok(None) => super::capability::PrivateAskAuthEvidence::observed(
            service.clone(),
            format!("{service}:staged-profile"),
            true,
        ),
        // Every refusal collapses to "unavailable" on purpose. The distinctions
        // are small oracles about this machine's secret store, and the viewer
        // acts on the same thing either way: authentication is not proven.
        Err(_) => super::capability::PrivateAskAuthEvidence::observed(
            service.clone(),
            format!("{service}:unavailable"),
            false,
        ),
    }
}

#[cfg(test)]
#[path = "credential_tests.rs"]
mod tests;
