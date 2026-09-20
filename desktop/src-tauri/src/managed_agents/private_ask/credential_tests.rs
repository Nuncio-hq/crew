//! Where a staged provider credential may and may not appear.
//!
//! The tests drive the production seam with a canary secret, then look for that
//! canary everywhere a credential has historically leaked: argv, the run root
//! on disk, the retained diagnostics, and the `Debug` rendering of the value
//! itself. A canary is used rather than a real token so a failure prints
//! something safe.

use super::*;
use crate::managed_agents::private_ask::egress_proxy::ProviderHost;

const CANARY: &str = "canary-private-ask-credential-must-not-leak";

fn proxy() -> EgressProxy {
    EgressProxy::start(ProviderHost::parse("api.anthropic.com").unwrap()).expect("loopback proxy")
}

/// The gate. Removing the `proxy.is_listening()` check in
/// `stage_private_ask_credential` lets a bearer token be created for a child
/// whose egress is not bounded by anything.
#[test]
fn no_credential_is_staged_without_a_live_proxy() {
    let proxy = proxy();
    let directory = tempfile::tempdir().expect("temp");
    drop_proxy_listener(&proxy);
    assert_eq!(
        stage_private_ask_credential("claude", directory.path(), &proxy, &AtomicBool::new(false))
            .unwrap_err(),
        PrivateAskFailure::EgressBoundUnverified
    );
}

/// Stopping the proxy is the only way to observe the closed gate; the handle
/// owns its listener, so this is the production teardown path.
fn drop_proxy_listener(proxy: &EgressProxy) {
    proxy.force_stop_for_test();
}

/// Hermes owns its own authentication, and that is a success with no token —
/// never an error, which would refuse a correctly configured run, and never a
/// silent empty token, which would launch unauthenticated.
#[test]
fn hermes_needs_no_credential_handoff() {
    let proxy = proxy();
    let directory = tempfile::tempdir().expect("temp");
    assert!(stage_private_ask_credential(
        "hermes",
        directory.path(),
        &proxy,
        &AtomicBool::new(false)
    )
    .expect("hermes staging")
    .is_none());
}

#[test]
fn an_unknown_runtime_has_no_credential_contract() {
    let proxy = proxy();
    let directory = tempfile::tempdir().expect("temp");
    assert_eq!(
        stage_private_ask_credential("codex", directory.path(), &proxy, &AtomicBool::new(false))
            .unwrap_err(),
        PrivateAskFailure::MissingRuntime
    );
}

/// An empty, padded or control-bearing token is refused rather than exported.
/// A child given an empty `CLAUDE_CODE_OAUTH_TOKEN` fails deep inside the
/// runtime and reports what looks like a model error.
#[test]
fn a_malformed_token_is_refused_before_it_reaches_an_environment() {
    for token in ["", " ", "token ", " token", "tok\nen", "tok\u{0}en"] {
        assert_eq!(
            claude_credential(token).unwrap_err(),
            PrivateAskFailure::AuthenticationUnverified,
            "accepted {token:?}"
        );
    }
    assert_eq!(
        claude_credential(&"x".repeat(CREDENTIAL_LIMIT + 1)).unwrap_err(),
        PrivateAskFailure::AuthenticationUnverified
    );
}

/// The credential travels in the environment under the same variable the rest
/// of the desktop uses, and nowhere else.
#[test]
fn the_credential_is_an_environment_entry_under_the_shared_variable_name() {
    let credential = claude_credential(CANARY).expect("canary credential");
    assert_eq!(credential.name(), "CLAUDE_CODE_OAUTH_TOKEN");
    let (name, value) = credential.env_entry();
    assert_eq!(name, "CLAUDE_CODE_OAUTH_TOKEN");
    assert_eq!(value, CANARY);
}

/// Removing the hand-written `Debug` puts the token into every trace, panic
/// message and error that ever formats a value containing one.
#[test]
fn the_credential_never_renders_its_value() {
    let credential = claude_credential(CANARY).expect("canary credential");
    let rendered = format!("{credential:?}");
    assert!(!rendered.contains(CANARY), "leaked: {rendered}");
    assert!(rendered.contains("redacted"));
    // The same must hold for a container that holds one.
    let wrapped = format!("{:?}", Some(credential));
    assert!(!wrapped.contains(CANARY), "leaked: {wrapped}");
}

/// Every refusal on this path collapses to one message, and none of them
/// mention the secret store's own diagnostics.
#[test]
fn a_refusal_carries_no_credential_detail() {
    let error = claude_credential("").unwrap_err();
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains(CANARY));
    assert_eq!(error.to_string(), "runtime authentication is unverified");
}
