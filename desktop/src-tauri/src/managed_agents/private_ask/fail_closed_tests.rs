//! Fail-closed behaviour of the containment resolver, on every platform.
//!
//! The macOS proof modules cannot run elsewhere, so this module carries the one
//! assertion every platform must still make: a run root that cannot be confined,
//! and a platform without this boundary at all, refuse a profile rather than
//! returning "no profile needed".

use super::containment::private_ask_containment_profile;
use super::PrivateAskFailure;
use std::path::Path;

#[test]
fn a_run_root_that_cannot_be_confined_refuses_a_containment_profile() {
    assert_eq!(
        private_ask_containment_profile(
            Path::new("relative/root"),
            Path::new("/usr/bin"),
            &[],
            41234
        ),
        Err(PrivateAskFailure::ProcessContainmentUnverified)
    );
    assert_eq!(
        private_ask_containment_profile(
            Path::new("/tmp/run\"root"),
            Path::new("/usr/bin"),
            &[],
            41234
        ),
        Err(PrivateAskFailure::ProcessContainmentUnverified)
    );
    assert_eq!(
        private_ask_containment_profile(
            Path::new("/tmp/run\\root"),
            Path::new("/usr/bin"),
            &[],
            41234
        ),
        Err(PrivateAskFailure::ProcessContainmentUnverified)
    );
}

/// A port of zero is not a bound listener, and SBPL would accept the literal
/// `localhost:0` without complaint. Removing the `proxy_port == 0` refusal in
/// `private_ask_containment_profile` lets a run start under a policy whose only
/// egress allowance points at nothing.
#[test]
fn a_policy_is_never_built_for_an_unbound_proxy_port() {
    assert_eq!(
        private_ask_containment_profile(
            Path::new("/tmp/well-formed-root"),
            Path::new("/usr/bin"),
            &[],
            0
        ),
        Err(PrivateAskFailure::ProcessContainmentUnverified)
    );
}

/// Off macOS there is no effect-denying boundary, so even a well-formed run root
/// must refuse. Removing the `Err` arm for other platforms fails this.
#[cfg(not(target_os = "macos"))]
#[test]
fn a_platform_without_this_boundary_never_returns_a_profile() {
    assert_eq!(
        private_ask_containment_profile(
            Path::new("/tmp/well-formed-root"),
            Path::new("/usr/bin"),
            &[],
            41234
        ),
        Err(PrivateAskFailure::ProcessContainmentUnverified)
    );
}
