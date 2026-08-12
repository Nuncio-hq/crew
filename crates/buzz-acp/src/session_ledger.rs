//! Durable per-thread ACP session ledger.
//!
//! Contract (issue #169):
//! 1. **Declare-at-birth** — write only after a successful `session/new`.
//! 2. **Resume-by-lookup-only** — wake may only resume the entry under the
//!    thread key; never invent or scan the engine store for "most recent".
//! 3. **Validate-then-load** — engine identity, workspace generation, and
//!    `loadSession` capability must match before `session/load`; any failure
//!    deletes the entry and rebuilds.
//!
//! Contract (issue #180 — compaction / rotation awareness):
//! 4. **Observe engine rotation** — Hermes `sessionProvenance.compressionDepth`
//!    / internal session ids, and Codex `compacted` / `context_compacted`
//!    markers, update `rotation_count` + optional `lineage` on the ledger.
//! 5. **Wake refuses stale lineage** — if the engine's rotation is ahead of
//!    the ledger (or the lineage tip mismatches), treat as a resume miss and
//!    fail closed to rebuild + delta delivery.
//!
//! Contract (issue #173 — owner-facing compaction aging):
//! 6. **Honest `compaction_count`** — increments only on real signals; unknown
//!    engines stay unknown (never fabricate a number). Persists beside
//!    `rotation_count`; resets on `session/new` declare (including OwnerReset).
//! 7. **Turn-count safety net** — `session_turn_count` ages signal-less engines
//!    without inventing a compaction number.
//!
//! Invariant: a thread owns a sequence of sessions over its lifetime; at most
//! one is live; only the newest is resumable; superseded sessions are history.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::compaction_signal::{
    apply_compaction_signal, mark_compaction_unavailable, project_session_aging,
    CompactionSignalAvailability, CompactionSignalSource, SessionAgingState,
    DEFAULT_COMPACTION_THRESHOLD, DEFAULT_TURN_AGING_THRESHOLD,
};
use crate::secure_spool::{
    ensure_secure_directory, read_secure_entry, remove_secure_entry, write_secure_entry_if_absent,
};

const LEDGER_EXT: &str = "json";
const MAX_ENTRY_BYTES: u64 = 64 * 1024;
/// Cap lineage history retained on disk (oldest tips drop first).
const MAX_LINEAGE_TIPS: usize = 32;

/// Live session declaration for one thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLedgerCurrent {
    pub session_id: String,
    /// Agent command + profile/store identity — blocks cross-engine loads.
    pub engine_identity: String,
    /// Must match `WorkspaceBinding.eviction_generation` (0 when unbound).
    pub workspace_generation: u64,
    pub created_at: u64,
    pub last_used_at: u64,
}

/// One observed engine-internal session tip after birth or compaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLineageTip {
    /// Engine-internal session id (Hermes `currentHermesSessionId`, Codex
    /// rollout tip, etc.). May differ from the ACP `sessionId` after rotate.
    pub internal_session_id: String,
    /// Observed compression / compaction depth at this tip.
    pub compression_depth: u32,
    /// Detector that produced this tip (`hermes.sessionProvenance`,
    /// `codex.context_compacted`, `codex.rollout_compacted`, …).
    pub source: String,
    pub observed_at: u64,
}

/// Why a ledger declare overwrote the previous live session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SessionDeclareReason {
    /// Ordinary `session/new` (wake rebuild, first turn, rotation).
    #[default]
    Birth,
    /// Owner-triggered guided / blind handover (#173).
    OwnerReset,
}

/// Durable ledger value for one `(relay, agent, thread)` key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLedgerEntry {
    pub current: SessionLedgerCurrent,
    /// Highest engine compression/compaction depth observed for the live ACP
    /// session. Resets to 0 (or the birth snapshot depth) on `session/new`
    /// declare. Never fabricated — only real engine signals move this.
    pub rotation_count: u32,
    /// Optional ordered tips (oldest → newest). Empty on legacy entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lineage: Vec<SessionLineageTip>,
    /// Owner-facing compaction counter (#173). Increments only on real signals;
    /// resets on every declare. Never shown unless `compaction_signal` is Known.
    #[serde(default)]
    pub compaction_count: u32,
    /// Whether a numeric compaction count may be shown.
    #[serde(default)]
    pub compaction_signal: CompactionSignalAvailability,
    /// Turns completed on this session generation (safety net for signal-less
    /// engines). Resets on declare.
    #[serde(default)]
    pub session_turn_count: u32,
    /// Last declare reason (birth vs owner reset). Legacy entries decode as Birth.
    #[serde(default)]
    pub declare_reason: SessionDeclareReason,
}

/// Lookup key for the durable ledger.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionLedgerKey {
    pub relay_url: String,
    pub agent_pubkey: String,
    pub thread_id: Uuid,
}

impl SessionLedgerKey {
    pub fn new(
        relay_url: impl Into<String>,
        agent_pubkey: impl Into<String>,
        thread_id: Uuid,
    ) -> Self {
        Self {
            relay_url: relay_url.into(),
            agent_pubkey: agent_pubkey.into().to_ascii_lowercase(),
            thread_id,
        }
    }

    fn file_stem(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.relay_url.as_bytes());
        hasher.update([0xff]);
        hasher.update(self.agent_pubkey.as_bytes());
        hasher.update([0xff]);
        hasher.update(self.thread_id.as_bytes());
        hex::encode(hasher.finalize())
    }

    fn entry_name(&self) -> OsString {
        OsString::from(format!("{}.{}", self.file_stem(), LEDGER_EXT))
    }

    fn temporary_name(&self) -> OsString {
        OsString::from(format!("{}.tmp", self.file_stem()))
    }
}

/// Snapshot of engine rotation state for wake-time comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineRotationSnapshot {
    pub compression_depth: u32,
    pub current_internal_id: String,
    pub source: &'static str,
}

/// Validation inputs required before / after `session/load`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLoadValidation<'a> {
    pub engine_identity: &'a str,
    pub workspace_generation: u64,
    pub load_session_supported: bool,
    /// When `Some`, compare against ledger `rotation_count` / lineage tip.
    /// Pass the snapshot parsed from `session/load` (or a Codex rollout probe).
    pub engine_rotation: Option<&'a EngineRotationSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionLoadDecision {
    Resume { session_id: String },
    Rebuild { reason: &'static str },
}

/// Root directory for the session ledger spool.
pub fn default_session_ledger_dir() -> PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join("buzz-acp-session-ledger-tests");
    }
    if let Some(path) = std::env::var_os("BUZZ_ACP_SESSION_LEDGER_DIR") {
        return PathBuf::from(path);
    }
    PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| ".".into()))
        .join(".local/share/nunciocrew/buzz-acp/session-ledger")
}

pub fn session_ledger_dir_for_scope(base: &Path, relay_url: &str, agent_pubkey: &str) -> PathBuf {
    let relay_hash = hex::encode(Sha256::digest(relay_url.as_bytes()));
    base.join(&relay_hash[..16.min(relay_hash.len())])
        .join(agent_pubkey.to_ascii_lowercase())
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Serialize without secrets — contract test pins the field set.
pub fn encode_ledger_entry(entry: &SessionLedgerEntry) -> Result<Vec<u8>, String> {
    serde_json::to_vec(entry).map_err(|error| format!("session ledger encode failed: {error}"))
}

pub fn decode_ledger_entry(bytes: &[u8]) -> Result<SessionLedgerEntry, String> {
    let entry: SessionLedgerEntry = serde_json::from_slice(bytes)
        .map_err(|error| format!("session ledger decode failed: {error}"))?;
    if entry.current.session_id.trim().is_empty() {
        return Err("session ledger entry missing session_id".into());
    }
    if entry.current.engine_identity.trim().is_empty() {
        return Err("session ledger entry missing engine_identity".into());
    }
    Ok(entry)
}

fn tip_from_snapshot(snapshot: &EngineRotationSnapshot, observed_at: u64) -> SessionLineageTip {
    SessionLineageTip {
        internal_session_id: snapshot.current_internal_id.clone(),
        compression_depth: snapshot.compression_depth,
        source: snapshot.source.to_string(),
        observed_at,
    }
}

fn push_lineage_tip(lineage: &mut Vec<SessionLineageTip>, tip: SessionLineageTip) {
    if lineage
        .last()
        .is_some_and(|last| last.internal_session_id == tip.internal_session_id)
    {
        // Same internal id — refresh depth/source/time on the tip.
        if let Some(last) = lineage.last_mut() {
            last.compression_depth = tip.compression_depth;
            last.source = tip.source;
            last.observed_at = tip.observed_at;
        }
        return;
    }
    lineage.push(tip);
    if lineage.len() > MAX_LINEAGE_TIPS {
        let drop_n = lineage.len() - MAX_LINEAGE_TIPS;
        lineage.drain(0..drop_n);
    }
}

async fn write_entry(
    dir: &Path,
    key: &SessionLedgerKey,
    entry: &SessionLedgerEntry,
) -> Result<(), String> {
    let bytes = encode_ledger_entry(entry)?;
    let name = key.entry_name();
    let _ = remove_secure_entry(dir, &name).await?;
    let written = write_secure_entry_if_absent(dir, &name, &key.temporary_name(), &bytes).await?;
    if !written {
        return Err("session ledger write raced with another writer".into());
    }
    Ok(())
}

/// Declare-at-birth / overwrite-on-Crew-rebuild write site.
///
/// A new ACP `session/new` resets rotation tracking and owner-facing
/// compaction / turn counters. Pass `initial_rotation` when `session/new` (or
/// an immediate probe) already reports provenance so the birth tip is recorded.
pub async fn declare_session(
    dir: &Path,
    key: &SessionLedgerKey,
    session_id: impl Into<String>,
    engine_identity: impl Into<String>,
    workspace_generation: u64,
    initial_rotation: Option<&EngineRotationSnapshot>,
) -> Result<SessionLedgerEntry, String> {
    declare_session_with_reason(
        dir,
        key,
        session_id,
        engine_identity,
        workspace_generation,
        initial_rotation,
        SessionDeclareReason::Birth,
    )
    .await
}

/// Declare with an explicit reason (OwnerReset for guided / blind handover).
pub async fn declare_session_with_reason(
    dir: &Path,
    key: &SessionLedgerKey,
    session_id: impl Into<String>,
    engine_identity: impl Into<String>,
    workspace_generation: u64,
    initial_rotation: Option<&EngineRotationSnapshot>,
    reason: SessionDeclareReason,
) -> Result<SessionLedgerEntry, String> {
    ensure_secure_directory(dir).await?;
    let now = now_unix_secs();
    let session_id = session_id.into();
    let mut lineage = Vec::new();
    let mut rotation_count = 0;
    let tip_source = match reason {
        SessionDeclareReason::Birth => "declare.birth",
        SessionDeclareReason::OwnerReset => "declare.owner_reset",
    };
    if let Some(snapshot) = initial_rotation {
        rotation_count = snapshot.compression_depth;
        push_lineage_tip(&mut lineage, tip_from_snapshot(snapshot, now));
    } else {
        // Seed a birth tip from the ACP session id so later lineage mismatch
        // checks have a baseline even when the engine emits no meta yet.
        push_lineage_tip(
            &mut lineage,
            SessionLineageTip {
                internal_session_id: session_id.clone(),
                compression_depth: 0,
                source: tip_source.into(),
                observed_at: now,
            },
        );
    }
    let entry = SessionLedgerEntry {
        current: SessionLedgerCurrent {
            session_id,
            engine_identity: engine_identity.into(),
            workspace_generation,
            created_at: now,
            last_used_at: now,
        },
        rotation_count,
        lineage,
        // Owner-facing counters always reset on session replacement (#173).
        compaction_count: 0,
        compaction_signal: CompactionSignalAvailability::Unknown,
        session_turn_count: 0,
        declare_reason: reason,
    };
    write_entry(dir, key, &entry).await?;
    Ok(entry)
}

/// Persist an observed engine compaction / session-rotation signal.
pub async fn record_rotation_signal(
    dir: &Path,
    key: &SessionLedgerKey,
    snapshot: &EngineRotationSnapshot,
) -> Result<Option<SessionLedgerEntry>, String> {
    let Some(mut entry) = read_ledger_entry(dir, key).await? else {
        return Ok(None);
    };
    let now = now_unix_secs();
    entry.rotation_count = entry.rotation_count.max(snapshot.compression_depth);
    push_lineage_tip(&mut entry.lineage, tip_from_snapshot(snapshot, now));
    let source = if snapshot.source.contains("rollout") {
        CompactionSignalSource::TranscriptMarker
    } else {
        CompactionSignalSource::AcpNotification
    };
    let (count, signal) = apply_compaction_signal(
        entry.compaction_count,
        entry.compaction_signal,
        source,
        Some(snapshot.compression_depth),
    );
    entry.compaction_count = count;
    entry.compaction_signal = signal;
    entry.current.last_used_at = now;
    write_entry(dir, key, &entry).await?;
    Ok(Some(entry))
}

/// Persist a hook-sourced compaction (buzz-agent `_PostCompact`, Claude glue).
pub async fn record_compaction_hook(
    dir: &Path,
    key: &SessionLedgerKey,
) -> Result<Option<SessionLedgerEntry>, String> {
    let Some(mut entry) = read_ledger_entry(dir, key).await? else {
        return Ok(None);
    };
    let (count, signal) = apply_compaction_signal(
        entry.compaction_count,
        entry.compaction_signal,
        CompactionSignalSource::Hook,
        None,
    );
    entry.compaction_count = count;
    entry.compaction_signal = signal;
    entry.current.last_used_at = now_unix_secs();
    write_entry(dir, key, &entry).await?;
    Ok(Some(entry))
}

/// Freeze owner-facing compaction into Unavailable after adapter parse failure.
#[allow(dead_code)] // wired when Codex rollout probe detects format drift
pub async fn record_compaction_unavailable(
    dir: &Path,
    key: &SessionLedgerKey,
) -> Result<Option<SessionLedgerEntry>, String> {
    let Some(mut entry) = read_ledger_entry(dir, key).await? else {
        return Ok(None);
    };
    let (count, signal) = mark_compaction_unavailable(entry.compaction_count);
    entry.compaction_count = count;
    entry.compaction_signal = signal;
    entry.current.last_used_at = now_unix_secs();
    write_entry(dir, key, &entry).await?;
    Ok(Some(entry))
}

/// Increment the durable per-session turn counter (aging safety net).
pub async fn record_session_turn(
    dir: &Path,
    key: &SessionLedgerKey,
) -> Result<Option<SessionLedgerEntry>, String> {
    let Some(mut entry) = read_ledger_entry(dir, key).await? else {
        return Ok(None);
    };
    entry.session_turn_count = entry.session_turn_count.saturating_add(1);
    entry.current.last_used_at = now_unix_secs();
    write_entry(dir, key, &entry).await?;
    Ok(Some(entry))
}

/// Project benign aging for an entry using configured thresholds.
pub fn aging_from_entry(
    entry: &SessionLedgerEntry,
    compaction_threshold: u32,
    turn_threshold: u32,
) -> SessionAgingState {
    project_session_aging(
        entry.compaction_count,
        entry.compaction_signal,
        entry.session_turn_count,
        compaction_threshold,
        turn_threshold,
    )
}

/// Default thresholds from env (`BUZZ_ACP_COMPACTION_THRESHOLD`,
/// `BUZZ_ACP_TURN_AGING_THRESHOLD`).
pub fn aging_thresholds_from_env() -> (u32, u32) {
    let compaction = std::env::var("BUZZ_ACP_COMPACTION_THRESHOLD")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(DEFAULT_COMPACTION_THRESHOLD);
    let turns = std::env::var("BUZZ_ACP_TURN_AGING_THRESHOLD")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(DEFAULT_TURN_AGING_THRESHOLD);
    (compaction, turns)
}

pub async fn touch_session_used(dir: &Path, key: &SessionLedgerKey) -> Result<(), String> {
    let Some(mut entry) = read_ledger_entry(dir, key).await? else {
        return Ok(());
    };
    entry.current.last_used_at = now_unix_secs();
    write_entry(dir, key, &entry).await?;
    Ok(())
}

pub async fn read_ledger_entry(
    dir: &Path,
    key: &SessionLedgerKey,
) -> Result<Option<SessionLedgerEntry>, String> {
    ensure_secure_directory(dir).await?;
    let Some(bytes) = read_secure_entry(dir, &key.entry_name(), MAX_ENTRY_BYTES).await? else {
        return Ok(None);
    };
    match decode_ledger_entry(&bytes) {
        Ok(entry) => Ok(Some(entry)),
        Err(error) => {
            tracing::warn!(
                target: "session_ledger",
                error = %error,
                "corrupt session ledger entry treated as absent"
            );
            let _ = remove_secure_entry(dir, &key.entry_name()).await;
            Ok(None)
        }
    }
}

pub async fn delete_ledger_entry(dir: &Path, key: &SessionLedgerKey) -> Result<bool, String> {
    ensure_secure_directory(dir).await?;
    remove_secure_entry(dir, &key.entry_name()).await
}

fn hermes_provenance(value: &Value) -> Option<&Value> {
    value
        .pointer("/_meta/hermes/sessionProvenance")
        .or_else(|| value.pointer("/params/update/_meta/hermes/sessionProvenance"))
        .or_else(|| value.pointer("/update/_meta/hermes/sessionProvenance"))
        .or_else(|| value.pointer("/hermes/sessionProvenance"))
}

fn parse_hermes_rotation(value: &Value) -> Option<EngineRotationSnapshot> {
    let prov = hermes_provenance(value)?;
    let current = prov
        .get("currentHermesSessionId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    let depth = prov
        .get("compressionDepth")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    Some(EngineRotationSnapshot {
        compression_depth: depth,
        current_internal_id: current.to_string(),
        source: "hermes.sessionProvenance",
    })
}

fn parse_codex_acp_rotation(value: &Value) -> Option<EngineRotationSnapshot> {
    // Live ACP path: session/update carrying context_compacted (when codex-acp
    // forwards it) or an explicit compacted marker under _meta.codex.
    let update = value
        .pointer("/params/update")
        .or_else(|| value.get("update"))
        .unwrap_or(value);

    let session_update = update.get("sessionUpdate").and_then(|v| v.as_str());
    let payload_type = update
        .pointer("/payload/type")
        .and_then(|v| v.as_str())
        .or_else(|| update.get("type").and_then(|v| v.as_str()));

    let is_compact = matches!(
        session_update,
        Some("context_compacted") | Some("compacted")
    ) || matches!(payload_type, Some("context_compacted") | Some("compacted"))
        || update
            .pointer("/_meta/codex/compacted")
            .and_then(|v| v.as_bool())
            == Some(true);

    if !is_compact {
        return None;
    }

    let session_id = value
        .pointer("/params/sessionId")
        .or_else(|| value.get("sessionId"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let depth = update
        .pointer("/_meta/codex/compactionCount")
        .or_else(|| update.get("compactionCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;

    Some(EngineRotationSnapshot {
        compression_depth: depth.max(1),
        current_internal_id: format!("{session_id}#compact-{depth}"),
        source: "codex.context_compacted",
    })
}

/// Parse a Hermes / Codex rotation signal from an ACP JSON value
/// (`session/new` result, `session/load` result, or `session/update`).
pub fn parse_engine_rotation_signal(value: &Value) -> Option<EngineRotationSnapshot> {
    parse_hermes_rotation(value).or_else(|| parse_codex_acp_rotation(value))
}

/// Count Codex rollout JSONL compaction markers (`type: compacted` or
/// `event_msg` / `context_compacted`). Returns `None` when the transcript
/// has no compaction evidence (caller must not fabricate a count).
///
/// Public probe surface for wake-time Codex detection when ACP does not
/// forward `context_compacted` (#180). Contract-tested; harness callers pass
/// the rollout bytes they already resolved from the engine store.
#[allow(dead_code)]
pub fn parse_codex_rollout_rotation(
    session_id: &str,
    jsonl: &str,
) -> Option<EngineRotationSnapshot> {
    let mut count = 0u32;
    for line in jsonl.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            // Fail loud into "unknown" — format drift must not undercount.
            return None;
        };
        let ty = value.get("type").and_then(|v| v.as_str());
        let payload_ty = value.pointer("/payload/type").and_then(|v| v.as_str());
        match (ty, payload_ty) {
            (Some("compacted"), _) => count = count.saturating_add(1),
            (Some("event_msg"), Some("context_compacted")) => count = count.saturating_add(1),
            _ => {}
        }
    }
    if count == 0 {
        return None;
    }
    Some(EngineRotationSnapshot {
        compression_depth: count,
        current_internal_id: format!("{session_id}#compact-{count}"),
        source: "codex.rollout_compacted",
    })
}

/// Resume-by-lookup-only + validate-then-load (+ optional lineage) decision.
pub fn decide_session_load(
    entry: Option<&SessionLedgerEntry>,
    validation: &SessionLoadValidation<'_>,
) -> SessionLoadDecision {
    let Some(entry) = entry else {
        return SessionLoadDecision::Rebuild {
            reason: "no ledger entry",
        };
    };
    if !validation.load_session_supported {
        return SessionLoadDecision::Rebuild {
            reason: "loadSession not advertised",
        };
    }
    if entry.current.engine_identity != validation.engine_identity {
        return SessionLoadDecision::Rebuild {
            reason: "engine identity mismatch",
        };
    }
    if entry.current.workspace_generation != validation.workspace_generation {
        return SessionLoadDecision::Rebuild {
            reason: "workspace generation mismatch",
        };
    }
    if entry.current.session_id.trim().is_empty() {
        return SessionLoadDecision::Rebuild {
            reason: "empty session id",
        };
    }
    if let Some(engine) = validation.engine_rotation {
        if entry.rotation_count < engine.compression_depth {
            return SessionLoadDecision::Rebuild {
                reason: "ledger rotation lags engine",
            };
        }
        match entry.lineage.last() {
            Some(tip) if tip.internal_session_id != engine.current_internal_id => {
                return SessionLoadDecision::Rebuild {
                    reason: "lineage mismatch",
                };
            }
            None if engine.compression_depth > 0 => {
                return SessionLoadDecision::Rebuild {
                    reason: "lineage mismatch",
                };
            }
            _ => {}
        }
    }
    SessionLoadDecision::Resume {
        session_id: entry.current.session_id.clone(),
    }
}

/// Build a stable engine identity string from command + args (no secrets).
pub fn engine_identity_from(command: &str, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(command.trim().to_string());
    for arg in args {
        let trimmed = arg.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Skip values that look like secrets / nsecs / tokens.
        if trimmed.starts_with("nsec1")
            || trimmed.contains("SECRET")
            || trimmed.contains("TOKEN")
            || trimmed.contains("API_KEY")
        {
            continue;
        }
        parts.push(trimmed.to_string());
    }
    parts.join("\0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_dir(label: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "buzz-acp-session-ledger-{}-{}-{}",
            label,
            std::process::id(),
            n
        ))
    }

    fn key() -> SessionLedgerKey {
        SessionLedgerKey::new(
            "ws://127.0.0.1:3000",
            "aa".repeat(32),
            Uuid::from_u128(0x1111_2222_3333_4444),
        )
    }

    fn base_validation<'a>(
        identity: &'a str,
        rotation: Option<&'a EngineRotationSnapshot>,
    ) -> SessionLoadValidation<'a> {
        SessionLoadValidation {
            engine_identity: identity,
            workspace_generation: 0,
            load_session_supported: true,
            engine_rotation: rotation,
        }
    }

    #[tokio::test]
    async fn declare_at_birth_resets_rotation_and_seeds_lineage() {
        let dir = unique_dir("declare");
        let key = key();
        let first = declare_session(&dir, &key, "sess-1", "hermes\0--profile\0a", 0, None)
            .await
            .unwrap();
        assert_eq!(first.rotation_count, 0);
        assert_eq!(first.current.session_id, "sess-1");
        assert_eq!(first.lineage.len(), 1);
        assert_eq!(first.lineage[0].internal_session_id, "sess-1");

        let second = declare_session(&dir, &key, "sess-2", "hermes\0--profile\0a", 0, None)
            .await
            .unwrap();
        assert_eq!(second.rotation_count, 0, "Crew rebuild resets rotation");
        assert_eq!(second.current.session_id, "sess-2");
        assert_eq!(second.lineage[0].internal_session_id, "sess-2");

        let loaded = read_ledger_entry(&dir, &key).await.unwrap().unwrap();
        assert_eq!(loaded.current.session_id, "sess-2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn resume_by_lookup_only_no_entry_rebuilds() {
        let decision = decide_session_load(None, &base_validation("hermes", None));
        assert_eq!(
            decision,
            SessionLoadDecision::Rebuild {
                reason: "no ledger entry"
            }
        );
    }

    #[tokio::test]
    async fn validate_then_load_rejects_identity_and_workspace_mismatch() {
        let entry = SessionLedgerEntry {
            current: SessionLedgerCurrent {
                session_id: "sess-1".into(),
                engine_identity: "hermes\0--profile\0a".into(),
                workspace_generation: 3,
                created_at: 1,
                last_used_at: 1,
            },
            rotation_count: 0,
            lineage: vec![],
            compaction_count: 0,
            compaction_signal: CompactionSignalAvailability::Unknown,
            session_turn_count: 0,
            declare_reason: SessionDeclareReason::Birth,
        };
        assert!(matches!(
            decide_session_load(
                Some(&entry),
                &SessionLoadValidation {
                    engine_identity: "codex",
                    workspace_generation: 3,
                    load_session_supported: true,
                    engine_rotation: None,
                }
            ),
            SessionLoadDecision::Rebuild {
                reason: "engine identity mismatch"
            }
        ));
        assert!(matches!(
            decide_session_load(
                Some(&entry),
                &SessionLoadValidation {
                    engine_identity: "hermes\0--profile\0a",
                    workspace_generation: 4,
                    load_session_supported: true,
                    engine_rotation: None,
                }
            ),
            SessionLoadDecision::Rebuild {
                reason: "workspace generation mismatch"
            }
        ));
        assert!(matches!(
            decide_session_load(
                Some(&entry),
                &SessionLoadValidation {
                    engine_identity: "hermes\0--profile\0a",
                    workspace_generation: 3,
                    load_session_supported: false,
                    engine_rotation: None,
                }
            ),
            SessionLoadDecision::Rebuild {
                reason: "loadSession not advertised"
            }
        ));
        assert_eq!(
            decide_session_load(
                Some(&entry),
                &SessionLoadValidation {
                    engine_identity: "hermes\0--profile\0a",
                    workspace_generation: 3,
                    load_session_supported: true,
                    engine_rotation: None,
                }
            ),
            SessionLoadDecision::Resume {
                session_id: "sess-1".into()
            }
        );
    }

    #[tokio::test]
    async fn corrupt_file_treated_as_absent() {
        let dir = unique_dir("corrupt");
        ensure_secure_directory(&dir).await.unwrap();
        let key = key();
        let name = key.entry_name();
        let tmp = key.temporary_name();
        write_secure_entry_if_absent(&dir, &name, &tmp, b"{not-json")
            .await
            .unwrap();
        let loaded = read_ledger_entry(&dir, &key).await.unwrap();
        assert_eq!(loaded, None);
        // Corrupt entry is removed so a later declare can succeed.
        let declared = declare_session(&dir, &key, "sess-new", "hermes", 0, None)
            .await
            .unwrap();
        assert_eq!(declared.current.session_id, "sess-new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn encoded_entry_contains_no_secret_field_names() {
        let entry = SessionLedgerEntry {
            current: SessionLedgerCurrent {
                session_id: "sess".into(),
                engine_identity: "hermes\0--profile\0ops".into(),
                workspace_generation: 0,
                created_at: 1,
                last_used_at: 2,
            },
            rotation_count: 1,
            lineage: vec![SessionLineageTip {
                internal_session_id: "internal-1".into(),
                compression_depth: 1,
                source: "hermes.sessionProvenance".into(),
                observed_at: 3,
            }],
            compaction_count: 1,
            compaction_signal: CompactionSignalAvailability::Known,
            session_turn_count: 4,
            declare_reason: SessionDeclareReason::Birth,
        };
        let json = String::from_utf8(encode_ledger_entry(&entry).unwrap()).unwrap();
        for forbidden in [
            "private_key",
            "privateKey",
            "nsec",
            "secret",
            "token",
            "password",
            "api_key",
            "apiKey",
        ] {
            assert!(
                !json.to_ascii_lowercase().contains(forbidden),
                "ledger JSON must not mention {forbidden}: {json}"
            );
        }
        assert!(json.contains("sessionId"));
        assert!(json.contains("engineIdentity"));
        assert!(json.contains("workspaceGeneration"));
        assert!(json.contains("rotationCount"));
        assert!(json.contains("lineage"));
        assert!(json.contains("internalSessionId"));
        assert!(json.contains("compressionDepth"));
    }

    #[test]
    fn engine_identity_skips_nsec_like_args() {
        let identity = engine_identity_from(
            "hermes-acp",
            &["--profile".into(), "ops".into(), "nsec1abc".into()],
        );
        assert_eq!(identity, "hermes-acp\0--profile\0ops");
        assert!(!identity.contains("nsec1"));
    }

    #[tokio::test]
    async fn delete_entry_scopes_cleanup() {
        let dir = unique_dir("delete");
        let key = key();
        declare_session(&dir, &key, "sess-1", "hermes", 0, None)
            .await
            .unwrap();
        assert!(delete_ledger_entry(&dir, &key).await.unwrap());
        assert_eq!(read_ledger_entry(&dir, &key).await.unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn multi_thread_session_ids_are_isolated() {
        let dir = unique_dir("threads");
        let thread_a = Uuid::from_u128(0xaaaa);
        let thread_b = Uuid::from_u128(0xbbbb);
        let thread_c = Uuid::from_u128(0xcccc);
        let key_a = SessionLedgerKey::new("ws://127.0.0.1:3000", "aa".repeat(32), thread_a);
        let key_b = SessionLedgerKey::new("ws://127.0.0.1:3000", "aa".repeat(32), thread_b);
        let key_c = SessionLedgerKey::new("ws://127.0.0.1:3000", "aa".repeat(32), thread_c);
        declare_session(&dir, &key_a, "sess-a", "hermes", 0, None)
            .await
            .unwrap();
        declare_session(&dir, &key_b, "sess-b", "hermes", 0, None)
            .await
            .unwrap();
        declare_session(&dir, &key_c, "sess-c", "hermes", 0, None)
            .await
            .unwrap();

        let a = read_ledger_entry(&dir, &key_a).await.unwrap().unwrap();
        let b = read_ledger_entry(&dir, &key_b).await.unwrap().unwrap();
        let c = read_ledger_entry(&dir, &key_c).await.unwrap().unwrap();
        assert_eq!(a.current.session_id, "sess-a");
        assert_eq!(b.current.session_id, "sess-b");
        assert_eq!(c.current.session_id, "sess-c");

        // Wake via A must only resume A's id.
        let decision = decide_session_load(Some(&a), &base_validation("hermes", None));
        assert_eq!(
            decision,
            SessionLoadDecision::Resume {
                session_id: "sess-a".into()
            }
        );
        assert_ne!(a.current.session_id, b.current.session_id);
        assert_ne!(a.current.session_id, c.current.session_id);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_hermes_session_provenance_from_session_info_update() {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": "acp-1",
                "update": {
                    "sessionUpdate": "session_info_update",
                    "_meta": {
                        "hermes": {
                            "sessionProvenance": {
                                "acpSessionId": "acp-1",
                                "currentHermesSessionId": "hermes-child-2",
                                "rootHermesSessionId": "acp-1",
                                "parentHermesSessionId": "hermes-child-1",
                                "sessionKind": "compression",
                                "compressionDepth": 2
                            }
                        }
                    }
                }
            }
        });
        let snap = parse_engine_rotation_signal(&msg).expect("hermes signal");
        assert_eq!(snap.compression_depth, 2);
        assert_eq!(snap.current_internal_id, "hermes-child-2");
        assert_eq!(snap.source, "hermes.sessionProvenance");
    }

    #[test]
    fn parse_codex_rollout_and_acp_compaction_signals() {
        let jsonl = r#"
{"type":"response_item","payload":{}}
{"type":"compacted","payload":{"replacement_history":[]}}
{"type":"event_msg","payload":{"type":"context_compacted"}}
"#;
        let snap = parse_codex_rollout_rotation("sess-x", jsonl).expect("codex rollout");
        assert_eq!(snap.compression_depth, 2);
        assert_eq!(snap.source, "codex.rollout_compacted");

        let acp = serde_json::json!({
            "params": {
                "sessionId": "sess-x",
                "update": {
                    "sessionUpdate": "context_compacted",
                    "_meta": { "codex": { "compactionCount": 3 } }
                }
            }
        });
        let acp_snap = parse_engine_rotation_signal(&acp).expect("codex acp");
        assert_eq!(acp_snap.compression_depth, 3);
        assert_eq!(acp_snap.source, "codex.context_compacted");
    }

    #[tokio::test]
    async fn rotate_then_wake_rebuilds_when_ledger_lags() {
        let dir = unique_dir("rotate-wake");
        let key = key();
        let entry = declare_session(&dir, &key, "acp-1", "hermes", 0, None)
            .await
            .unwrap();
        assert_eq!(entry.rotation_count, 0);

        // Engine compacted twice; ledger never observed it (missed signal).
        let engine = EngineRotationSnapshot {
            compression_depth: 2,
            current_internal_id: "hermes-child-2".into(),
            source: "hermes.sessionProvenance",
        };
        let decision = decide_session_load(Some(&entry), &base_validation("hermes", Some(&engine)));
        assert_eq!(
            decision,
            SessionLoadDecision::Rebuild {
                reason: "ledger rotation lags engine"
            }
        );

        // After observation, matching wake may resume.
        let updated = record_rotation_signal(&dir, &key, &engine)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.rotation_count, 2);
        assert_eq!(
            updated.lineage.last().unwrap().internal_session_id,
            "hermes-child-2"
        );
        let ok = decide_session_load(Some(&updated), &base_validation("hermes", Some(&engine)));
        assert_eq!(
            ok,
            SessionLoadDecision::Resume {
                session_id: "acp-1".into()
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn lineage_mismatch_rebuilds_even_when_depth_matches() {
        let dir = unique_dir("lineage-mismatch");
        let key = key();
        let birth = EngineRotationSnapshot {
            compression_depth: 1,
            current_internal_id: "hermes-a".into(),
            source: "hermes.sessionProvenance",
        };
        let entry = declare_session(&dir, &key, "acp-1", "hermes", 0, Some(&birth))
            .await
            .unwrap();
        let other = EngineRotationSnapshot {
            compression_depth: 1,
            current_internal_id: "hermes-OTHER".into(),
            source: "hermes.sessionProvenance",
        };
        assert_eq!(
            decide_session_load(Some(&entry), &base_validation("hermes", Some(&other))),
            SessionLoadDecision::Rebuild {
                reason: "lineage mismatch"
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn multi_thread_lineage_is_isolated() {
        let dir = unique_dir("lineage-threads");
        let key_a = SessionLedgerKey::new(
            "ws://127.0.0.1:3000",
            "aa".repeat(32),
            Uuid::from_u128(0xaaaa),
        );
        let key_b = SessionLedgerKey::new(
            "ws://127.0.0.1:3000",
            "aa".repeat(32),
            Uuid::from_u128(0xbbbb),
        );
        declare_session(&dir, &key_a, "sess-a", "hermes", 0, None)
            .await
            .unwrap();
        declare_session(&dir, &key_b, "sess-b", "hermes", 0, None)
            .await
            .unwrap();

        let rot_a = EngineRotationSnapshot {
            compression_depth: 2,
            current_internal_id: "internal-a-2".into(),
            source: "hermes.sessionProvenance",
        };
        let rot_b = EngineRotationSnapshot {
            compression_depth: 1,
            current_internal_id: "internal-b-1".into(),
            source: "hermes.sessionProvenance",
        };
        record_rotation_signal(&dir, &key_a, &rot_a).await.unwrap();
        record_rotation_signal(&dir, &key_b, &rot_b).await.unwrap();

        let a = read_ledger_entry(&dir, &key_a).await.unwrap().unwrap();
        let b = read_ledger_entry(&dir, &key_b).await.unwrap().unwrap();
        assert_eq!(a.rotation_count, 2);
        assert_eq!(b.rotation_count, 1);
        assert_eq!(
            a.lineage.last().unwrap().internal_session_id,
            "internal-a-2"
        );
        assert_eq!(
            b.lineage.last().unwrap().internal_session_id,
            "internal-b-1"
        );

        // Wake A with B's lineage must rebuild — never resume across threads.
        assert_eq!(
            decide_session_load(Some(&a), &base_validation("hermes", Some(&rot_b))),
            SessionLoadDecision::Rebuild {
                reason: "lineage mismatch"
            }
        );
        assert_eq!(
            decide_session_load(Some(&a), &base_validation("hermes", Some(&rot_a))),
            SessionLoadDecision::Resume {
                session_id: "sess-a".into()
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_entry_without_lineage_field_decodes() {
        let json = br#"{"current":{"sessionId":"s","engineIdentity":"hermes","workspaceGeneration":0,"createdAt":1,"lastUsedAt":1},"rotationCount":0}"#;
        let entry = decode_ledger_entry(json).unwrap();
        assert!(entry.lineage.is_empty());
        assert_eq!(entry.rotation_count, 0);
        assert_eq!(entry.compaction_count, 0);
        assert_eq!(
            entry.compaction_signal,
            CompactionSignalAvailability::Unknown
        );
        assert_eq!(entry.session_turn_count, 0);
    }

    #[tokio::test]
    async fn compaction_count_only_moves_on_real_signal_and_resets_on_declare() {
        let dir = unique_dir("compaction-honesty");
        let key = key();
        let entry = declare_session(&dir, &key, "acp-1", "hermes", 0, None)
            .await
            .unwrap();
        assert_eq!(entry.compaction_count, 0);
        assert_eq!(
            entry.compaction_signal,
            CompactionSignalAvailability::Unknown
        );

        let snap = EngineRotationSnapshot {
            compression_depth: 3,
            current_internal_id: "h-3".into(),
            source: "hermes.sessionProvenance",
        };
        let updated = record_rotation_signal(&dir, &key, &snap)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.compaction_count, 3);
        assert_eq!(
            updated.compaction_signal,
            CompactionSignalAvailability::Known
        );
        assert!(aging_from_entry(&updated, 3, 100).aging);

        let reset = declare_session_with_reason(
            &dir,
            &key,
            "acp-2",
            "hermes",
            0,
            None,
            SessionDeclareReason::OwnerReset,
        )
        .await
        .unwrap();
        assert_eq!(reset.compaction_count, 0);
        assert_eq!(
            reset.compaction_signal,
            CompactionSignalAvailability::Unknown
        );
        assert_eq!(reset.session_turn_count, 0);
        assert_eq!(reset.declare_reason, SessionDeclareReason::OwnerReset);
        assert!(!aging_from_entry(&reset, 3, 100).aging);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn turn_count_net_ages_unknown_signal_without_fabricating_count() {
        let dir = unique_dir("turn-net");
        let key = key();
        declare_session(&dir, &key, "acp-1", "grok", 0, None)
            .await
            .unwrap();
        for _ in 0..100 {
            record_session_turn(&dir, &key).await.unwrap();
        }
        let entry = read_ledger_entry(&dir, &key).await.unwrap().unwrap();
        assert_eq!(entry.session_turn_count, 100);
        assert_eq!(
            entry.compaction_signal,
            CompactionSignalAvailability::Unknown
        );
        let aging = aging_from_entry(&entry, 3, 100);
        assert!(aging.aging);
        assert_eq!(aging.reason, Some("turn_count_net"));
        assert_eq!(aging.compaction_count, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn post_compact_hook_increments_honest_count() {
        let dir = unique_dir("post-compact");
        let key = key();
        declare_session(&dir, &key, "acp-1", "buzz-agent", 0, None)
            .await
            .unwrap();
        let once = record_compaction_hook(&dir, &key).await.unwrap().unwrap();
        assert_eq!(once.compaction_count, 1);
        assert_eq!(once.compaction_signal, CompactionSignalAvailability::Known);
        let twice = record_compaction_hook(&dir, &key).await.unwrap().unwrap();
        assert_eq!(twice.compaction_count, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn corrupt_transcript_marks_unavailable_not_undercount() {
        let dir = unique_dir("unavailable");
        let key = key();
        declare_session(&dir, &key, "acp-1", "codex", 0, None)
            .await
            .unwrap();
        let snap = EngineRotationSnapshot {
            compression_depth: 1,
            current_internal_id: "c-1".into(),
            source: "codex.context_compacted",
        };
        record_rotation_signal(&dir, &key, &snap).await.unwrap();
        let frozen = record_compaction_unavailable(&dir, &key)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            frozen.compaction_signal,
            CompactionSignalAvailability::Unavailable
        );
        let snap2 = EngineRotationSnapshot {
            compression_depth: 9,
            current_internal_id: "c-9".into(),
            source: "codex.context_compacted",
        };
        let after = record_rotation_signal(&dir, &key, &snap2)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.compaction_count, 1, "unavailable must not miscount");
        assert_eq!(
            after.compaction_signal,
            CompactionSignalAvailability::Unavailable
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
