//! The effect-denying Seatbelt policy for one private Ask run root.
//!
//! The recap policy (`allow default` plus `deny process-fork`) contains the
//! process tree but denies neither file effects nor network egress: a runtime
//! that still has a write or HTTP tool can act on the machine even though no
//! descendant can be forked. A private Ask carries the viewer's question into a
//! full employee identity, so the boundary here denies the *effects* as well.
//!
//! Policy shape, verified on macOS 25.5 before it was written here:
//!
//! * `allow default` is the policy's default action, not an ordered rule —
//!   placing it after a deny does not re-permit the denied operation.
//! * Among ordered rules the later, more specific match wins, so the run-root
//!   `file-write*` allowance must follow the blanket `file-write*` denial.
//! * `remote ip "*:443"` keeps the provider HTTPS path reachable. SBPL does not
//!   resolve hostnames, so a host-scoped rule is not available; the mDNSResponder
//!   socket is allowed separately because name resolution is a local UNIX socket
//!   connect, which `deny network-outbound` would otherwise refuse.
//! * `process-exec*` is deliberately *not* denied: `sandbox-exec` applies the
//!   policy and then `execvp`s the runtime itself, so denying exec makes the
//!   launch fail outright. `deny process-fork` is what stops new processes; an
//!   in-place `execve` inherits this same policy and escapes nothing.

use super::PrivateAskFailure;
use std::path::Path;

/// Build the Seatbelt policy text confining one attempt to `run_root`.
///
/// Returns [`PrivateAskFailure::ProcessContainmentUnverified`] on any platform
/// where this boundary cannot be applied. There is deliberately no "no profile
/// needed" success value: a missing boundary must stop the launch, not pass
/// through it.
pub(super) fn private_ask_containment_profile(
    run_root: &Path,
) -> Result<String, PrivateAskFailure> {
    #[cfg(target_os = "macos")]
    {
        if !run_root.is_absolute() {
            return Err(PrivateAskFailure::ProcessContainmentUnverified);
        }
        // A quoted SBPL string has no escape for `"` or `\`; a run root
        // containing either would end the literal early and silently widen the
        // policy. The owned run root is a UUID below the managed-agent base, so
        // this rejects only a caller that bypassed `OwnedRecapRun`.
        let root = run_root
            .to_str()
            .ok_or(PrivateAskFailure::ProcessContainmentUnverified)?;
        if root.contains('"') || root.contains('\\') || root.contains('\n') {
            return Err(PrivateAskFailure::ProcessContainmentUnverified);
        }
        if !std::fs::metadata("/usr/bin/sandbox-exec").is_ok_and(|metadata| metadata.is_file()) {
            return Err(PrivateAskFailure::ProcessContainmentUnverified);
        }
        Ok(format!(
            "(version 1)\n\
             (allow default)\n\
             (deny file-write*)\n\
             (allow file-write* (subpath \"{root}\"))\n\
             (allow file-write-data (literal \"/dev/null\") (literal \"/dev/dtracehelper\"))\n\
             (deny network-outbound)\n\
             (allow network-outbound (remote ip \"*:443\"))\n\
             (allow network-outbound (literal \"/private/var/run/mDNSResponder\"))\n\
             (deny process-fork)"
        ))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = run_root;
        Err(PrivateAskFailure::ProcessContainmentUnverified)
    }
}
