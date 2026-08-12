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
//! Invariant: a thread owns a sequence of sessions over its lifetime; at most
//! one is live; only the newest is resumable; superseded sessions are history.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::secure_spool::{
    ensure_secure_directory, read_secure_entry, remove_secure_entry, write_secure_entry_if_absent,
};

const LEDGER_EXT: &str = "json";
const MAX_ENTRY_BYTES: u64 = 64 * 1024;

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

/// Durable ledger value for one `(relay, agent, thread)` key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLedgerEntry {
    pub current: SessionLedgerCurrent,
    pub rotation_count: u32,
}

/// Lookup key for the durable ledger.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionLedgerKey {
    pub relay_url: String,
    pub agent_pubkey: String,
    pub thread_id: Uuid,
}

impl SessionLedgerKey {
    pub fn new(relay_url: impl Into<String>, agent_pubkey: impl Into<String>, thread_id: Uuid) -> Self {
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

/// Validation inputs required before `session/load`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLoadValidation<'a> {
    pub engine_identity: &'a str,
    pub workspace_generation: u64,
    pub load_session_supported: bool,
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

/// Declare-at-birth / overwrite-on-rotation write site.
pub async fn declare_session(
    dir: &Path,
    key: &SessionLedgerKey,
    session_id: impl Into<String>,
    engine_identity: impl Into<String>,
    workspace_generation: u64,
) -> Result<SessionLedgerEntry, String> {
    ensure_secure_directory(dir).await?;
    let name = key.entry_name();
    let previous = read_ledger_entry(dir, key).await?;
    let now = now_unix_secs();
    let entry = SessionLedgerEntry {
        current: SessionLedgerCurrent {
            session_id: session_id.into(),
            engine_identity: engine_identity.into(),
            workspace_generation,
            created_at: now,
            last_used_at: now,
        },
        rotation_count: previous.map(|p| p.rotation_count.saturating_add(1)).unwrap_or(0),
    };
    let bytes = encode_ledger_entry(&entry)?;
    // Replace: remove any prior entry, then exclusive-create.
    let _ = remove_secure_entry(dir, &name).await?;
    let written =
        write_secure_entry_if_absent(dir, &name, &key.temporary_name(), &bytes).await?;
    if !written {
        return Err("session ledger declare raced with another writer".into());
    }
    Ok(entry)
}

pub async fn touch_session_used(dir: &Path, key: &SessionLedgerKey) -> Result<(), String> {
    let Some(mut entry) = read_ledger_entry(dir, key).await? else {
        return Ok(());
    };
    entry.current.last_used_at = now_unix_secs();
    let bytes = encode_ledger_entry(&entry)?;
    let name = key.entry_name();
    let _ = remove_secure_entry(dir, &name).await?;
    let _ = write_secure_entry_if_absent(dir, &name, &key.temporary_name(), &bytes).await?;
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

/// Resume-by-lookup-only + validate-then-load decision.
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

    #[tokio::test]
    async fn declare_at_birth_writes_and_overwrites_on_rotation() {
        let dir = unique_dir("declare");
        let key = key();
        let first = declare_session(&dir, &key, "sess-1", "hermes\0--profile\0a", 0)
            .await
            .unwrap();
        assert_eq!(first.rotation_count, 0);
        assert_eq!(first.current.session_id, "sess-1");

        let second = declare_session(&dir, &key, "sess-2", "hermes\0--profile\0a", 0)
            .await
            .unwrap();
        assert_eq!(second.rotation_count, 1);
        assert_eq!(second.current.session_id, "sess-2");

        let loaded = read_ledger_entry(&dir, &key).await.unwrap().unwrap();
        assert_eq!(loaded.current.session_id, "sess-2");
        assert_eq!(loaded.rotation_count, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn resume_by_lookup_only_no_entry_rebuilds() {
        let decision = decide_session_load(
            None,
            &SessionLoadValidation {
                engine_identity: "hermes",
                workspace_generation: 0,
                load_session_supported: true,
            },
        );
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
        };
        assert!(matches!(
            decide_session_load(
                Some(&entry),
                &SessionLoadValidation {
                    engine_identity: "codex",
                    workspace_generation: 3,
                    load_session_supported: true,
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
        let declared = declare_session(&dir, &key, "sess-new", "hermes", 0)
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
            rotation_count: 0,
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
        declare_session(&dir, &key, "sess-1", "hermes", 0)
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
        declare_session(&dir, &key_a, "sess-a", "hermes", 0)
            .await
            .unwrap();
        declare_session(&dir, &key_b, "sess-b", "hermes", 0)
            .await
            .unwrap();
        declare_session(&dir, &key_c, "sess-c", "hermes", 0)
            .await
            .unwrap();

        let a = read_ledger_entry(&dir, &key_a).await.unwrap().unwrap();
        let b = read_ledger_entry(&dir, &key_b).await.unwrap().unwrap();
        let c = read_ledger_entry(&dir, &key_c).await.unwrap().unwrap();
        assert_eq!(a.current.session_id, "sess-a");
        assert_eq!(b.current.session_id, "sess-b");
        assert_eq!(c.current.session_id, "sess-c");

        // Wake via A must only resume A's id.
        let decision = decide_session_load(
            Some(&a),
            &SessionLoadValidation {
                engine_identity: "hermes",
                workspace_generation: 0,
                load_session_supported: true,
            },
        );
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
}
