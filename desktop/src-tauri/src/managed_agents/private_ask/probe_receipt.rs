//! The durable side of a capability probe.
//!
//! Probing is expensive — it launches a contained process — so one trace serves
//! every Ask for the same selection until it expires. That means the trace
//! outlives the process that captured it, and a file on disk becomes an input
//! to a capability decision. This module is the only place that happens, and it
//! is deliberately narrow.
//!
//! Three things keep it honest:
//!
//! * **Ownership.** The receipt lives in an owned, uid-validated, 0o700
//!   directory and is written 0o600 through a temporary file and a rename, so a
//!   torn write is never readable as a receipt. It carries this install's
//!   ownership digest, exactly as `RuntimeReadyDocument` does, so a receipt
//!   copied from another machine — or kept across a re-provision — no longer
//!   matches and is refused.
//! * **No free construction.** The evidence types have no `Deserialize`. This
//!   module owns plain data-transfer structs and rebuilds the real types
//!   through named constructors, so "a JSON file" never becomes "any capability
//!   a caller cares to describe".
//! * **Nothing is trusted because it was persisted.** Loading a receipt yields
//!   a [`PrivateAskProbe`], not a capability. Every fence
//!   `PrivateAskCapability::from_probe` applies to a trace captured moments ago
//!   — freshness against `PROBE_MAX_AGE`, the rebuilt containment profile, the
//!   selection, the probe program digest, `bounds_egress` — applies unchanged
//!   to one read from disk. Expiry is what forces a fresh probe.
//!
//! NAMED LIMIT: a receipt deliberately does **not** carry session-isolation
//! evidence. `independent_invocation` is a statement about one attempt running
//! beside one live session, not a property of a machine that can be cached; a
//! loaded receipt therefore leaves it unverified and the attempt must observe
//! it for itself.

use super::capability::{
    PrivateAskAuthEvidence, PrivateAskProbe, PrivateAskToolProbe, PROBE_MAX_AGE,
};
use super::egress_proxy::{EgressObservation, RefusalReason};
use super::PrivateAskFailure;
use crate::managed_agents::recap_capability::RecapExecutableIdentity;
use crate::managed_agents::recap_ownership::VerifiedStagingOwnership;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};

const RECEIPT_SCHEMA: &str = "crew-private-ask-probe-receipt";
const RECEIPT_VERSION: u8 = 1;

/// Bytes accepted from one receipt file. A receipt is a few kilobytes of
/// scalars; anything larger is not one, and is refused before it is parsed.
const RECEIPT_LIMIT: u64 = 256 * 1024;

/// Which selection a receipt describes, as a filename.
///
/// The key is the part that must match for a receipt to be *about* this
/// runtime at all: the runtime, the exact bytes of its executable, and the
/// probe program that produced the trace. Everything finer — model, profile,
/// persona, ACL, session generation — is re-checked by `from_probe`, which
/// rejects a mismatch as evidence about something else rather than silently
/// adapting it.
///
/// The executable fingerprint is in the key rather than only in the body so an
/// upgraded runtime does not overwrite, or read back, the old binary's trace:
/// it simply has no receipt and re-probes.
fn receipt_key(executable: &RecapExecutableIdentity, probe_program_digest: &str) -> String {
    let mut hasher = Sha256::new();
    for part in [
        executable.fingerprint.as_str(),
        executable.platform.as_str(),
        probe_program_digest,
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    hex::encode(hasher.finalize())
}

fn receipt_path(base: &Path, runtime_id: &str, key: &str) -> Result<PathBuf, PrivateAskFailure> {
    // The runtime id reaches a filename, so it is checked rather than trusted.
    // Only the two runtimes this feature supports are ever written.
    if !matches!(runtime_id, "claude" | "hermes") {
        return Err(PrivateAskFailure::MissingRuntime);
    }
    Ok(base.join(format!("{runtime_id}-{key}.json")))
}

/// Persist one freshly captured trace.
///
/// Overwriting an existing receipt is the normal case — re-probing replaces
/// what it supersedes. The write is temp-file-plus-rename so a reader never
/// sees a half-written receipt, and a crash mid-write leaves the previous one
/// intact rather than a truncated file that would parse as nothing.
pub(super) fn store(
    ownership: &VerifiedStagingOwnership,
    probe: &PrivateAskProbe,
) -> Result<(), PrivateAskFailure> {
    let base = ownership
        .private_ask_probe_base()
        .map_err(PrivateAskFailure::State)?;
    let key = receipt_key(&probe.executable, &probe.probe_program_digest);
    let path = receipt_path(&base, &probe.runtime_id, &key)?;
    let document = ReceiptDocument::from_probe(ownership, probe);
    let bytes = serde_json::to_vec(&document).map_err(|_| PrivateAskFailure::InvalidState)?;

    let temporary = base.join(format!(".{key}-{}.tmp", uuid::Uuid::new_v4()));
    let mut file = private_new_file(&temporary)?;
    let written = file
        .write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| PrivateAskFailure::InvalidState);
    drop(file);
    if let Err(failure) = written {
        let _ = std::fs::remove_file(&temporary);
        return Err(failure);
    }
    if std::fs::rename(&temporary, &path).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err(PrivateAskFailure::InvalidState);
    }
    Ok(())
}

/// Read back the retained trace for one selection, if there is a usable one.
///
/// `None` covers every "there is nothing to use here" case and they are
/// deliberately not distinguished: no file, an unreadable or oversized file,
/// unparseable bytes, a receipt written by another install, a receipt for
/// another uid, and a receipt past [`PROBE_MAX_AGE`]. The caller's response to
/// all of them is the same — probe again — and a refusal that named which one
/// would be a small oracle about this machine's state.
///
/// Expiry is checked here as well as in `from_probe` so a stale receipt is
/// replaced rather than repeatedly loaded and rejected.
pub(super) fn load(
    ownership: &VerifiedStagingOwnership,
    executable: &RecapExecutableIdentity,
    runtime_id: &str,
    probe_program_digest: &str,
    now: u64,
) -> Option<PrivateAskProbe> {
    let base = ownership.private_ask_probe_base().ok()?;
    let key = receipt_key(executable, probe_program_digest);
    let path = receipt_path(&base, runtime_id, &key).ok()?;

    let metadata = std::fs::symlink_metadata(&path).ok()?;
    // A symlink here would let something outside the owned tree answer for a
    // receipt; only a regular file is read.
    if !metadata.is_file() || metadata.len() > RECEIPT_LIMIT {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != ownership.owner_uid() {
            return None;
        }
    }
    let bytes = std::fs::read(&path).ok()?;
    let document: ReceiptDocument = serde_json::from_slice(&bytes).ok()?;
    if document.schema != RECEIPT_SCHEMA
        || document.version != RECEIPT_VERSION
        || document.ownership_sha256 != ownership.ownership_digest()
        || document.owner_uid != ownership.owner_uid()
        || document.probe_program_digest != probe_program_digest
        || document.runtime_id != runtime_id
    {
        return None;
    }
    // A stamp from the future is not a reading of this machine's clock, and a
    // stamp past the window no longer describes this machine at all.
    if document.captured_at == 0
        || document.captured_at > now
        || now.saturating_sub(document.captured_at) > PROBE_MAX_AGE
    {
        return None;
    }
    document.into_probe()
}

#[cfg(unix)]
fn private_new_file(path: &Path) -> Result<std::fs::File, PrivateAskFailure> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| PrivateAskFailure::InvalidState)
}

#[cfg(not(unix))]
fn private_new_file(path: &Path) -> Result<std::fs::File, PrivateAskFailure> {
    let _ = path;
    Err(PrivateAskFailure::InvalidState)
}

/// Plain data only.
///
/// The evidence types deliberately carry no `Deserialize`, so this is the sole
/// shape JSON can take on the way in, and [`ReceiptDocument::into_probe`] is
/// the sole way it becomes evidence again.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptDocument {
    schema: String,
    version: u8,
    ownership_sha256: String,
    owner_uid: u32,
    runtime_id: String,
    executable: ExecutableDocument,
    effective_model: String,
    profile: Option<String>,
    persona: String,
    acl_fingerprint: String,
    session_generation: String,
    auth: AuthDocument,
    tool_probe: ToolProbeDocument,
    containment_profile: String,
    probe_run_root: PathBuf,
    egress: EgressDocument,
    captured_at: u64,
    run_nonce: String,
    staging_base: PathBuf,
    external_state_before: String,
    external_state_after: String,
    probe_program_digest: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutableDocument {
    resolved_path: PathBuf,
    version: String,
    fingerprint: String,
    platform: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthDocument {
    service: String,
    reference: String,
    auth_available: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolProbeDocument {
    probe_id: String,
    tool_name: String,
    request_observed: bool,
    denied_before_effect: bool,
    sentinel_before: String,
    sentinel_after: String,
    read_outside_requested: bool,
    read_outside_denied: bool,
    network_connections_observed: u32,
    surviving_descendants: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EgressDocument {
    provider_host: String,
    proxy_port: u16,
    accepted: Vec<String>,
    refused: Vec<RefusedDocument>,
    dial_failures: u32,
    truncated: bool,
    direct_connections: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RefusedDocument {
    target: String,
    reason: String,
}

/// The refusal reason travels as a fixed string rather than a serde-derived
/// enum, so an unknown value from a future build is refused instead of being
/// mapped onto whichever variant happens to sort first.
fn reason_label(reason: RefusalReason) -> &'static str {
    match reason {
        RefusalReason::NotConnect => "not-connect",
        RefusalReason::ForeignHost => "foreign-host",
        RefusalReason::ForeignPort => "foreign-port",
        RefusalReason::TunnelCap => "tunnel-cap",
        RefusalReason::SniMismatch => "sni-mismatch",
        RefusalReason::NotTls => "not-tls",
    }
}

fn reason_from_label(label: &str) -> Option<RefusalReason> {
    match label {
        "not-connect" => Some(RefusalReason::NotConnect),
        "foreign-host" => Some(RefusalReason::ForeignHost),
        "foreign-port" => Some(RefusalReason::ForeignPort),
        "tunnel-cap" => Some(RefusalReason::TunnelCap),
        "sni-mismatch" => Some(RefusalReason::SniMismatch),
        "not-tls" => Some(RefusalReason::NotTls),
        _ => None,
    }
}

impl ReceiptDocument {
    fn from_probe(ownership: &VerifiedStagingOwnership, probe: &PrivateAskProbe) -> Self {
        Self {
            schema: RECEIPT_SCHEMA.to_owned(),
            version: RECEIPT_VERSION,
            ownership_sha256: ownership.ownership_digest().to_owned(),
            owner_uid: ownership.owner_uid(),
            runtime_id: probe.runtime_id.clone(),
            executable: ExecutableDocument {
                resolved_path: probe.executable.resolved_path.clone(),
                version: probe.executable.version.clone(),
                fingerprint: probe.executable.fingerprint.clone(),
                platform: probe.executable.platform.clone(),
            },
            effective_model: probe.effective_model.clone(),
            profile: probe.profile.clone(),
            persona: probe.persona.clone(),
            acl_fingerprint: probe.acl_fingerprint.clone(),
            session_generation: probe.session_generation.clone(),
            auth: AuthDocument {
                service: probe.auth.service().to_owned(),
                reference: probe.auth.reference().to_owned(),
                auth_available: probe.auth.auth_available(),
            },
            tool_probe: ToolProbeDocument {
                probe_id: probe.tool_probe.probe_id.clone(),
                tool_name: probe.tool_probe.tool_name.clone(),
                request_observed: probe.tool_probe.request_observed,
                denied_before_effect: probe.tool_probe.denied_before_effect,
                sentinel_before: probe.tool_probe.sentinel_before.clone(),
                sentinel_after: probe.tool_probe.sentinel_after.clone(),
                read_outside_requested: probe.tool_probe.read_outside_requested,
                read_outside_denied: probe.tool_probe.read_outside_denied,
                network_connections_observed: probe.tool_probe.network_connections_observed,
                surviving_descendants: probe.tool_probe.surviving_descendants,
            },
            containment_profile: probe.containment_profile.clone(),
            probe_run_root: probe.probe_run_root.clone(),
            egress: EgressDocument {
                provider_host: probe.egress.provider_host().to_owned(),
                proxy_port: probe.egress.proxy_port(),
                accepted: probe.egress.accepted().to_vec(),
                refused: probe
                    .egress
                    .refused()
                    .iter()
                    .map(|refused| RefusedDocument {
                        target: refused.target().to_owned(),
                        reason: reason_label(refused.reason()).to_owned(),
                    })
                    .collect(),
                dial_failures: probe.egress.dial_failures(),
                truncated: probe.egress.truncated(),
                direct_connections: probe.egress.direct_connections(),
            },
            captured_at: probe.captured_at,
            run_nonce: probe.run_nonce.clone(),
            staging_base: probe.staging_base.clone(),
            external_state_before: probe.external_state_before.clone(),
            external_state_after: probe.external_state_after.clone(),
            probe_program_digest: probe.probe_program_digest.clone(),
        }
    }

    /// Rebuild the trace, or refuse.
    ///
    /// `None` rather than a partially populated probe: a receipt carrying a
    /// refusal reason this build does not know is not evidence this build can
    /// reason about, and guessing a variant would change what
    /// `bounds_egress` concludes.
    fn into_probe(self) -> Option<PrivateAskProbe> {
        let mut refused = Vec::with_capacity(self.egress.refused.len());
        for entry in self.egress.refused {
            refused.push((entry.target, reason_from_label(&entry.reason)?));
        }
        Some(PrivateAskProbe {
            runtime_id: self.runtime_id,
            executable: RecapExecutableIdentity {
                resolved_path: self.executable.resolved_path,
                version: self.executable.version,
                fingerprint: self.executable.fingerprint,
                platform: self.executable.platform,
            },
            effective_model: self.effective_model,
            profile: self.profile,
            persona: self.persona,
            acl_fingerprint: self.acl_fingerprint,
            session_generation: self.session_generation,
            auth: PrivateAskAuthEvidence::observed(
                self.auth.service,
                self.auth.reference,
                self.auth.auth_available,
            ),
            tool_probe: PrivateAskToolProbe {
                probe_id: self.tool_probe.probe_id,
                tool_name: self.tool_probe.tool_name,
                request_observed: self.tool_probe.request_observed,
                denied_before_effect: self.tool_probe.denied_before_effect,
                sentinel_before: self.tool_probe.sentinel_before,
                sentinel_after: self.tool_probe.sentinel_after,
                read_outside_requested: self.tool_probe.read_outside_requested,
                read_outside_denied: self.tool_probe.read_outside_denied,
                network_connections_observed: self.tool_probe.network_connections_observed,
                surviving_descendants: self.tool_probe.surviving_descendants,
            },
            containment_profile: self.containment_profile,
            probe_run_root: self.probe_run_root,
            egress: EgressObservation::from_receipt(
                self.egress.provider_host,
                self.egress.proxy_port,
                self.egress.accepted,
                refused,
                self.egress.dial_failures,
                self.egress.truncated,
                self.egress.direct_connections,
            ),
            captured_at: self.captured_at,
            run_nonce: self.run_nonce,
            staging_base: self.staging_base,
            external_state_before: self.external_state_before,
            external_state_after: self.external_state_after,
            // See the module note: independence is per-attempt evidence and is
            // never restored from a receipt.
            session_isolation: None,
            probe_program_digest: self.probe_program_digest,
        })
    }
}

#[cfg(test)]
#[path = "probe_receipt_tests.rs"]
mod tests;
