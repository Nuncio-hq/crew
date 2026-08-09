//! ACP form elicitation normalization and answer reconstruction.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use buzz_core::{
    kind::{
        KIND_AGENT_USER_INPUT_ANSWER, KIND_AGENT_USER_INPUT_REQUESTED,
        KIND_AGENT_USER_INPUT_RESOLVED,
    },
    user_input::{
        Engine, Option_, UserInputAnswer, UserInputAnswers, UserInputQuestion, UserInputRequest,
        UserInputResolutionOutcome, UserInputResolved, UserInputSelection,
    },
};
use nostr::Keys;
use tokio::sync::{oneshot, watch, Mutex};
#[cfg(test)]
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use crate::relay::{BuzzEvent, RelayEventPublisher, RestClient};
use crate::secure_spool::{
    ensure_secure_directory, lock_secure_directory, lock_secure_entry_lease,
    measure_secure_directory, read_secure_entries, read_secure_entry, remove_secure_entry,
    rename_secure_entry, write_secure_entry_if_absent, SecureSpoolEntryLease,
    SECURE_SPOOL_LOCK_CONTENDED,
};
use crate::OwnerCache;

const RESOLUTION_RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];
const RESOLUTION_OUTBOX_RETRY_DELAY: Duration = Duration::from_secs(30);
#[cfg(not(test))]
const REQUEST_ADMISSION_RETRY_DELAY: Duration = Duration::from_secs(30);
#[cfg(test)]
const REQUEST_ADMISSION_RETRY_DELAY: Duration = Duration::from_millis(10);
#[cfg(not(test))]
const REQUEST_ADMISSION_INLINE_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(test)]
const REQUEST_ADMISSION_INLINE_TIMEOUT: Duration = Duration::from_millis(5);
const RESOLUTION_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const FILESYSTEM_RETRY_ATTEMPTS: usize = 3;
#[cfg(not(test))]
const FILESYSTEM_RETRY_DELAY: Duration = Duration::from_secs(1);
#[cfg(test)]
const FILESYSTEM_RETRY_DELAY: Duration = Duration::from_millis(10);
const MAX_SPOOL_ENTRIES: usize = 4_096;
const MAX_SPOOL_BYTES: u64 = 64 * 1024 * 1024;
static RESOLUTION_OUTBOX_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn resolution_outbox_lock() -> &'static Mutex<()> {
    RESOLUTION_OUTBOX_LOCK.get_or_init(|| Mutex::new(()))
}

fn resolution_outbox_dir(runtime: &QuestionRuntime) -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};

    #[cfg(test)]
    let base = std::fs::canonicalize(std::env::temp_dir())
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("buzz-acp-resolution-outbox-tests");
    #[cfg(not(test))]
    let base = match std::env::var_os("BUZZ_ACP_RESOLUTION_OUTBOX_DIR") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(std::env::var_os("HOME").ok_or_else(|| {
            "HOME or BUZZ_ACP_RESOLUTION_OUTBOX_DIR is required for resolution durability"
                .to_string()
        })?)
        .join(".local/share/nunciocrew/buzz-acp/resolution-outbox"),
    };
    let relay_hash = hex::encode(Sha256::digest(runtime.rest_client.base_url.as_bytes()));
    Ok(base
        .join(&relay_hash[..16])
        .join(runtime.keys.public_key().to_hex()))
}

fn pending_request_outbox_dir(runtime: &QuestionRuntime) -> Result<PathBuf, String> {
    Ok(resolution_outbox_dir(runtime)?.join("pending-requests"))
}

fn pending_request_lease_name(request_event_id: &str) -> String {
    format!("{request_event_id}.lease")
}

async fn acquire_pending_request_lease(
    runtime: &QuestionRuntime,
    request_event_id: &str,
    reserve_request_entry: bool,
) -> Result<SecureSpoolEntryLease, String> {
    let capacity_root = resolution_outbox_dir(runtime)?;
    let pending_directory = pending_request_outbox_dir(runtime)?;
    ensure_secure_spool_dir(&capacity_root).await?;
    ensure_secure_spool_dir(&pending_directory).await?;
    let _capacity_lock = lock_secure_directory(&capacity_root).await?;
    acquire_pending_request_lease_under_capacity_lock(
        runtime,
        request_event_id,
        reserve_request_entry,
    )
    .await
}

async fn acquire_pending_request_lease_under_capacity_lock(
    runtime: &QuestionRuntime,
    request_event_id: &str,
    reserve_request_entry: bool,
) -> Result<SecureSpoolEntryLease, String> {
    let capacity_root = resolution_outbox_dir(runtime)?;
    let pending_directory = pending_request_outbox_dir(runtime)?;
    let lease_name = pending_request_lease_name(request_event_id);
    let lease_exists = read_secure_entry(&pending_directory, lease_name.as_ref(), 0)
        .await?
        .is_some();
    let request_exists = if reserve_request_entry {
        read_secure_entry(
            &pending_directory,
            format!("{request_event_id}.json").as_ref(),
            MAX_SPOOL_BYTES,
        )
        .await?
        .is_some()
    } else {
        true
    };
    let additional_entries = usize::from(!lease_exists) + usize::from(!request_exists);
    ensure_spool_capacity(&capacity_root, additional_entries, 0).await?;
    lock_secure_entry_lease(&pending_directory, lease_name.as_ref()).await
}

async fn ensure_secure_spool_dir(path: &Path) -> Result<(), String> {
    ensure_secure_directory(path).await
}

async fn ensure_spool_capacity(
    path: &Path,
    additional_entries: usize,
    additional_bytes: u64,
) -> Result<(), String> {
    let (count, bytes) = measure_secure_directory(path, MAX_SPOOL_BYTES).await?;
    if count.saturating_add(additional_entries) > MAX_SPOOL_ENTRIES
        || bytes.saturating_add(additional_bytes) > MAX_SPOOL_BYTES
    {
        return Err(format!(
            "durable spool capacity exceeded ({count} entries, {bytes} bytes)"
        ));
    }
    Ok(())
}

async fn validate_spool_capacity(path: &Path) -> Result<(), String> {
    let (count, bytes) = measure_secure_directory(path, MAX_SPOOL_BYTES).await?;
    if count > MAX_SPOOL_ENTRIES || bytes > MAX_SPOOL_BYTES {
        return Err(format!(
            "durable spool capacity exceeded ({count} entries, {bytes} bytes)"
        ));
    }
    Ok(())
}

async fn persist_outbox_event(outbox_dir: &Path, event: &nostr::Event) -> Result<PathBuf, String> {
    let capacity_root =
        if outbox_dir.file_name().and_then(|value| value.to_str()) == Some("pending-requests") {
            let parent = outbox_dir
                .parent()
                .ok_or_else(|| "pending request outbox has no parent spool".to_string())?;
            ensure_secure_spool_dir(parent).await?;
            parent
        } else {
            outbox_dir
        };
    ensure_secure_spool_dir(outbox_dir).await?;
    let _capacity_lock = lock_secure_directory(capacity_root).await?;
    let file_name = format!("{}.json", event.id);
    let path = outbox_dir.join(&file_name);
    if let Some(existing) =
        read_secure_entry(outbox_dir, file_name.as_ref(), MAX_SPOOL_BYTES).await?
    {
        let existing: nostr::Event = serde_json::from_slice(&existing)
            .map_err(|error| format!("invalid existing resolution outbox entry: {error}"))?;
        if existing != *event {
            return Err(
                "existing resolution outbox entry does not match exact signed event".into(),
            );
        }
        return Ok(path);
    }
    let bytes = serde_json::to_vec(event)
        .map_err(|error| format!("failed to encode resolution outbox entry: {error}"))?;
    ensure_spool_capacity(capacity_root, 1, bytes.len() as u64).await?;
    let temporary_name = format!("{}.{}.tmp", event.id, Uuid::new_v4());
    if !write_secure_entry_if_absent(
        outbox_dir,
        std::ffi::OsStr::new(&file_name),
        std::ffi::OsStr::new(&temporary_name),
        &bytes,
    )
    .await?
    {
        let existing = read_secure_entry(
            outbox_dir,
            std::ffi::OsStr::new(&file_name),
            MAX_SPOOL_BYTES,
        )
        .await?
        .ok_or_else(|| "durable spool entry vanished during exact-event commit".to_string())?;
        let existing: nostr::Event = serde_json::from_slice(&existing)
            .map_err(|error| format!("invalid raced resolution outbox entry: {error}"))?;
        if existing != *event {
            return Err("raced resolution outbox entry does not match exact signed event".into());
        }
    }
    Ok(path)
}

async fn dead_letter_outbox_once(path: &Path, label: &str) -> Result<PathBuf, String> {
    let rejected = path.with_extension("rejected");
    let directory = path
        .parent()
        .ok_or_else(|| format!("{label} durable outbox entry has no parent directory"))?;
    let source_name = path
        .file_name()
        .ok_or_else(|| format!("{label} durable outbox entry has no filename"))?;
    let rejected_name = rejected
        .file_name()
        .ok_or_else(|| format!("{label} durable rejected entry has no filename"))?;
    let _guard = resolution_outbox_lock().lock().await;
    match rename_secure_entry(directory, source_name, rejected_name).await {
        Ok(true) => Ok(rejected),
        Ok(false) => match read_secure_entry(directory, rejected_name, MAX_SPOOL_BYTES).await {
            Ok(Some(_)) => Ok(rejected),
            Ok(None) => Err("source and rejected durable entries are both absent".to_owned()),
            Err(error) => Err(error),
        },
        Err(error) => Err(error),
    }
}

async fn dead_letter_outbox_until_committed(path: &Path, label: &str) -> Option<PathBuf> {
    for attempt in 1..=FILESYSTEM_RETRY_ATTEMPTS {
        match dead_letter_outbox_once(path, label).await {
            Ok(rejected) => return Some(rejected),
            Err(error) => {
                if attempt == FILESYSTEM_RETRY_ATTEMPTS {
                    tracing::error!(%error, path = %path.display(), label, "leaving durable entry queued after bounded dead-letter retries");
                    break;
                }
                tracing::error!(%error, path = %path.display(), label, attempt, "retrying durable dead-letter move");
                tokio::time::sleep(FILESYSTEM_RETRY_DELAY).await;
            }
        }
    }
    None
}

async fn retire_pending_request(
    runtime: &QuestionRuntime,
    request_event_id: &str,
) -> Result<(), String> {
    let capacity_root = resolution_outbox_dir(runtime)?;
    let directory = pending_request_outbox_dir(runtime)?;
    let _capacity_lock = lock_secure_directory(&capacity_root).await?;
    let file_name = format!("{request_event_id}.json");
    remove_secure_entry(&directory, file_name.as_ref()).await?;
    let lease_name = pending_request_lease_name(request_event_id);
    remove_secure_entry(&directory, lease_name.as_ref()).await?;
    Ok(())
}

async fn retire_acknowledged_resolution(
    runtime: &QuestionRuntime,
    request_event_id: &str,
    resolution_path: &Path,
) -> bool {
    for attempt in 1..=FILESYSTEM_RETRY_ATTEMPTS {
        match retire_pending_request(runtime, request_event_id).await {
            Ok(()) => break,
            Err(error) => {
                if attempt == FILESYSTEM_RETRY_ATTEMPTS {
                    tracing::error!(%error, request_event_id, "retaining acknowledged resolution after bounded request cleanup retries");
                    return false;
                }
                tracing::error!(%error, request_event_id, attempt, "retaining acknowledged resolution until request cleanup succeeds");
                tokio::time::sleep(FILESYSTEM_RETRY_DELAY).await;
            }
        }
    }
    for attempt in 1..=FILESYSTEM_RETRY_ATTEMPTS {
        let result = {
            let _guard = resolution_outbox_lock().lock().await;
            let Some(directory) = resolution_path.parent() else {
                return false;
            };
            let Some(name) = resolution_path.file_name() else {
                return false;
            };
            remove_secure_entry(directory, name).await
        };
        match result {
            Ok(true) | Ok(false) => break,
            Err(error) => {
                if attempt == FILESYSTEM_RETRY_ATTEMPTS {
                    tracing::error!(%error, path = %resolution_path.display(), "retaining acknowledged resolution after bounded cleanup retries");
                    return false;
                }
                tracing::error!(%error, path = %resolution_path.display(), attempt, "retrying acknowledged resolution cleanup");
                tokio::time::sleep(FILESYSTEM_RETRY_DELAY).await;
            }
        }
    }
    true
}

async fn dead_letter_pending_request(runtime: &QuestionRuntime, request_event_id: &str) -> bool {
    let Ok(capacity_root) = resolution_outbox_dir(runtime) else {
        return false;
    };
    let Ok(directory) = pending_request_outbox_dir(runtime) else {
        return false;
    };
    let path = directory.join(format!("{request_event_id}.json"));
    let lease_name = pending_request_lease_name(request_event_id);
    for attempt in 1..=FILESYSTEM_RETRY_ATTEMPTS {
        let result = async {
            let _capacity_lock = lock_secure_directory(&capacity_root).await?;
            dead_letter_outbox_once(&path, "pending user-input request").await?;
            remove_secure_entry(&directory, lease_name.as_ref()).await?;
            Ok::<(), String>(())
        }
        .await;
        match result {
            Ok(()) => return true,
            Err(error) if attempt == FILESYSTEM_RETRY_ATTEMPTS => {
                tracing::error!(%error, request_event_id, "failed to dead-letter pending user-input request under the root recovery lock");
            }
            Err(error) => {
                tracing::error!(%error, request_event_id, attempt, "retrying pending user-input dead-letter under the root recovery lock");
                tokio::time::sleep(FILESYSTEM_RETRY_DELAY).await;
            }
        }
    }
    false
}

async fn quarantine_recovery_entry(directory: &Path, name: &std::ffi::OsStr, label: &str) -> bool {
    let invalid_name = Path::new(name).with_extension("invalid");
    match rename_secure_entry(directory, name, invalid_name.as_os_str()).await {
        Ok(true) => {
            tracing::error!(entry = %name.to_string_lossy(), label, "quarantined invalid durable recovery entry");
            true
        }
        Ok(false) => {
            tracing::error!(entry = %name.to_string_lossy(), label, "invalid durable recovery entry vanished before quarantine");
            false
        }
        Err(error) => {
            tracing::error!(%error, entry = %name.to_string_lossy(), label, "failed to quarantine invalid durable recovery entry");
            false
        }
    }
}

async fn wait_resolution_recovery_delay(_runtime: &QuestionRuntime) -> bool {
    #[cfg(test)]
    {
        tokio::select! {
            _ = _runtime.test_worker_cancel.cancelled() => false,
            _ = tokio::time::sleep(RESOLUTION_OUTBOX_RETRY_DELAY) => true,
        }
    }
    #[cfg(not(test))]
    {
        tokio::time::sleep(RESOLUTION_OUTBOX_RETRY_DELAY).await;
        true
    }
}

fn resolution_request_event_id(event: &nostr::Event) -> Option<String> {
    if event.kind.as_u16() as u32 != KIND_AGENT_USER_INPUT_RESOLVED {
        return None;
    }
    let request_event_id = serde_json::from_str::<UserInputResolved>(&event.content)
        .ok()?
        .request_event_id;
    (single_relationship_tag(event, "e") == Some(request_event_id.as_str())
        && single_relationship_tag(event, "h").is_some()
        && single_relationship_tag(event, "p").is_some())
    .then_some(request_event_id)
}

/// Engine field mapping retained while a form is pending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldMapping {
    /// Crew question identifier.
    pub id: String,
    /// Native ACP content key.
    pub field_key: String,
    /// Native custom-answer key, when present.
    pub custom_key: Option<String>,
    /// Whether the native field accepts an array.
    pub multi_select: bool,
    /// Whether the native field is required by the engine schema.
    pub required: bool,
}

/// Normalized ACP form and its native field mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedForm {
    /// Client-facing questions.
    pub questions: Vec<UserInputQuestion>,
    /// Native reconstruction map.
    pub mappings: Vec<FieldMapping>,
}

/// A published elicitation whose answer is still pending.
pub(crate) struct PendingQuestion {
    /// ACP JSON-RPC request identifier.
    pub request_id: serde_json::Value,
    /// Normalized form used to rebuild the engine response.
    pub form: NormalizedForm,
    /// Receiver resolved by the relay answer router or cancellation.
    pub receiver: oneshot::Receiver<Result<Option<UserInputAnswers>, String>>,
}

/// Shared transport for durable user-input requests and owner-authored answers.
pub(crate) struct QuestionRuntime {
    #[cfg(test)]
    test_publisher: RelayEventPublisher,
    keys: Keys,
    owner_cache: Arc<OwnerCache>,
    rest_client: RestClient,
    pending: Mutex<HashMap<String, PendingRequest>>,
    workers: TaskTracker,
    #[cfg(test)]
    test_worker_cancel: CancellationToken,
}

struct PendingRequest {
    channel_id: Uuid,
    intended_owner_pubkey: String,
    sender: oneshot::Sender<Result<Option<UserInputAnswers>, String>>,
    admission_tx: watch::Sender<bool>,
    resolution_started: bool,
    _lease: Option<SecureSpoolEntryLease>,
}

fn answer_author_is_intended_owner(author: &str, intended_owner_pubkey: &str) -> bool {
    author == intended_owner_pubkey
}

fn single_relationship_tag<'a>(event: &'a nostr::Event, name: &str) -> Option<&'a str> {
    let mut tags = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|value| value == name));
    let tag = tags.next()?;
    if tags.next().is_some() {
        return None;
    }
    tag.as_slice()
        .get(1)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
}

impl QuestionRuntime {
    pub(crate) fn new(
        publisher: RelayEventPublisher,
        keys: Keys,
        owner_cache: Arc<OwnerCache>,
        rest_client: RestClient,
    ) -> Arc<Self> {
        #[cfg(not(test))]
        let _ = publisher;
        Arc::new(Self {
            #[cfg(test)]
            test_publisher: publisher,
            keys,
            owner_cache,
            rest_client,
            pending: Mutex::new(HashMap::new()),
            workers: TaskTracker::new(),
            #[cfg(test)]
            test_worker_cancel: CancellationToken::new(),
        })
    }

    #[cfg(test)]
    async fn stop_test_workers(&self) {
        self.test_worker_cancel.cancel();
        self.workers.close();
        self.workers.wait().await;
    }

    /// Resume exact-signed resolutions that were persisted before a prior
    pub(crate) async fn resume_resolution_outbox(self: &Arc<Self>) -> bool {
        let runtime = Arc::clone(self);
        let outbox_dir = match resolution_outbox_dir(&runtime) {
            Ok(path) => path,
            Err(error) => {
                tracing::error!(%error, "cannot resume durable user-input resolution outbox");
                return false;
            }
        };
        if let Err(error) = ensure_secure_spool_dir(&outbox_dir).await {
            tracing::error!(%error, path = %outbox_dir.display(), "cannot secure durable user-input resolution outbox");
            return false;
        }
        let _capacity_lock = match lock_secure_directory(&outbox_dir).await {
            Ok(lock) => lock,
            Err(error) => {
                tracing::error!(%error, path = %outbox_dir.display(), "cannot claim durable user-input recovery spool");
                return false;
            }
        };
        if let Err(error) = validate_spool_capacity(&outbox_dir).await {
            tracing::error!(%error, path = %outbox_dir.display(), "unsafe resolution outbox entry blocks startup recovery");
            return false;
        }
        let mut recovery_complete = true;
        let mut resolving_requests = HashSet::new();
        let resolution_entries = match read_secure_entries(&outbox_dir, "json", MAX_SPOOL_BYTES)
            .await
        {
            Ok(entries) => entries,
            Err(error) => {
                tracing::error!(%error, path = %outbox_dir.display(), "failed closed while reading user-input resolution outbox");
                return false;
            }
        };
        let mut recovered_resolutions = Vec::with_capacity(resolution_entries.len());
        for entry in resolution_entries {
            let path = outbox_dir.join(&entry.name);
            let event = match serde_json::from_slice::<nostr::Event>(&entry.bytes) {
                Ok(event)
                    if event.pubkey == runtime.keys.public_key() && event.verify().is_ok() =>
                {
                    event
                }
                Ok(_) | Err(_) => {
                    recovery_complete &= quarantine_recovery_entry(
                        &outbox_dir,
                        &entry.name,
                        "invalid user-input resolution",
                    )
                    .await;
                    continue;
                }
            };
            let Some(request_event_id) = resolution_request_event_id(&event) else {
                recovery_complete &= quarantine_recovery_entry(
                    &outbox_dir,
                    &entry.name,
                    "user-input resolution without request authority",
                )
                .await;
                continue;
            };
            if !resolving_requests.insert(request_event_id.clone()) {
                recovery_complete &= quarantine_recovery_entry(
                    &outbox_dir,
                    &entry.name,
                    "duplicate terminal user-input resolution",
                )
                .await;
                continue;
            }
            recovered_resolutions.push((event, path, request_event_id));
        }

        let pending_directory = match pending_request_outbox_dir(&runtime) {
            Ok(directory) => directory,
            Err(error) => {
                tracing::error!(%error, "cannot resume durable pending user-input requests");
                return false;
            }
        };
        if let Err(error) = ensure_secure_spool_dir(&pending_directory).await {
            tracing::error!(%error, path = %pending_directory.display(), "cannot secure pending user-input request ledger");
            return false;
        }
        if let Err(error) = validate_spool_capacity(&outbox_dir).await {
            tracing::error!(%error, path = %outbox_dir.display(), "unsafe paired user-input ledger blocks startup recovery");
            return false;
        }
        let pending_entries = match read_secure_entries(&pending_directory, "json", MAX_SPOOL_BYTES)
            .await
        {
            Ok(entries) => entries,
            Err(error) => {
                tracing::error!(%error, path = %pending_directory.display(), "failed closed while reading pending user-input request ledger");
                return false;
            }
        };
        let mut paired_requests = HashMap::new();
        let mut orphaned_requests = Vec::new();
        let mut live_request_ids = HashSet::new();
        for entry in pending_entries {
            let path = pending_directory.join(&entry.name);
            let Some(request_event_id) = Path::new(&entry.name)
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_owned)
            else {
                recovery_complete &= quarantine_recovery_entry(
                    &pending_directory,
                    &entry.name,
                    "invalid pending request filename",
                )
                .await;
                continue;
            };
            let event = match serde_json::from_slice::<nostr::Event>(&entry.bytes) {
                Ok(event)
                    if event.id.to_hex() == request_event_id
                        && event.kind.as_u16() as u32 == KIND_AGENT_USER_INPUT_REQUESTED
                        && event.pubkey == self.keys.public_key()
                        && event.verify().is_ok() =>
                {
                    event
                }
                Ok(_) | Err(_) => {
                    recovery_complete &= quarantine_recovery_entry(
                        &pending_directory,
                        &entry.name,
                        "invalid pending user-input request",
                    )
                    .await;
                    continue;
                }
            };
            let Some(channel_id) =
                single_relationship_tag(&event, "h").and_then(|value| Uuid::parse_str(value).ok())
            else {
                recovery_complete &= quarantine_recovery_entry(
                    &pending_directory,
                    &entry.name,
                    "pending request without channel authority",
                )
                .await;
                continue;
            };
            let Some(owner_pubkey) = single_relationship_tag(&event, "p").map(str::to_owned) else {
                recovery_complete &= quarantine_recovery_entry(
                    &pending_directory,
                    &entry.name,
                    "pending request without owner authority",
                )
                .await;
                continue;
            };
            let request_matches_channel = serde_json::from_str::<UserInputRequest>(&event.content)
                .is_ok_and(|request| request.channel_id == channel_id.to_string());
            if !request_matches_channel {
                recovery_complete &= quarantine_recovery_entry(
                    &pending_directory,
                    &entry.name,
                    "pending request content mismatch",
                )
                .await;
                continue;
            }
            let request_lease = match acquire_pending_request_lease_under_capacity_lock(
                &runtime,
                &request_event_id,
                false,
            )
            .await
            {
                Ok(lease) => lease,
                Err(error) if error == SECURE_SPOOL_LOCK_CONTENDED => {
                    live_request_ids.insert(request_event_id);
                    recovery_complete = false;
                    continue;
                }
                Err(error) => {
                    tracing::error!(%error, path = %path.display(), "failed closed while claiming pending request recovery lease");
                    recovery_complete = false;
                    continue;
                }
            };
            if resolving_requests.contains(&request_event_id) {
                paired_requests.insert(request_event_id, (event, path, request_lease));
                continue;
            }
            orphaned_requests.push((event, path, channel_id, owner_pubkey, request_lease));
        }

        let mut ready_resolutions = Vec::new();
        for (event, path, request_event_id) in recovered_resolutions {
            if live_request_ids.contains(&request_event_id) {
                continue;
            }
            let Some((request_event, _request_path, request_lease)) =
                paired_requests.remove(&request_event_id)
            else {
                let Some(name) = path.file_name() else {
                    recovery_complete = false;
                    continue;
                };
                recovery_complete &= quarantine_recovery_entry(
                    &outbox_dir,
                    name,
                    "terminal resolution without paired request ledger",
                )
                .await;
                continue;
            };
            ready_resolutions.push((event, path, request_event_id, request_event, request_lease));
        }
        drop(_capacity_lock);
        for (event, path, request_event_id, request_event, request_lease) in ready_resolutions {
            runtime.spawn_recovered_resolution(
                event,
                path,
                request_event_id,
                Some(request_event),
                Some(request_lease),
            );
        }
        for (event, path, channel_id, owner_pubkey, request_lease) in orphaned_requests {
            runtime.spawn_orphaned_request_cancellation(
                event,
                path,
                channel_id,
                owner_pubkey,
                request_lease,
            );
        }
        recovery_complete
    }

    pub(crate) fn retry_resolution_outbox(self: &Arc<Self>) {
        let runtime = Arc::clone(self);
        self.workers.spawn(async move {
            loop {
                tokio::time::sleep(RESOLUTION_OUTBOX_RETRY_DELAY).await;
                if runtime.resume_resolution_outbox().await {
                    break;
                }
            }
        });
    }

    fn retry_pending_dead_letter(self: &Arc<Self>, request_event_id: String) {
        let runtime = Arc::clone(self);
        self.workers.spawn(async move {
            while wait_resolution_recovery_delay(&runtime).await {
                if dead_letter_pending_request(&runtime, &request_event_id).await {
                    break;
                }
            }
        });
    }

    fn retry_incomplete_pending_request_retirement(self: &Arc<Self>, request_event_id: String) {
        let runtime = Arc::clone(self);
        self.workers.spawn(async move {
            loop {
                match retire_pending_request(&runtime, &request_event_id).await {
                    Ok(()) => break,
                    Err(error) => tracing::warn!(
                        %error,
                        request_event_id,
                        "retrying incomplete pending request retirement"
                    ),
                }
                if !wait_resolution_recovery_delay(&runtime).await {
                    break;
                }
            }
        });
    }

    fn retry_acknowledged_resolution_retirement(
        self: &Arc<Self>,
        request_event_id: String,
        resolution_path: PathBuf,
    ) {
        let runtime = Arc::clone(self);
        self.workers.spawn(async move {
            while wait_resolution_recovery_delay(&runtime).await {
                if retire_acknowledged_resolution(&runtime, &request_event_id, &resolution_path)
                    .await
                {
                    break;
                }
            }
        });
    }

    fn spawn_recovered_resolution(
        self: &Arc<Self>,
        event: nostr::Event,
        path: PathBuf,
        request_event_id: String,
        request_event: Option<nostr::Event>,
        request_lease: Option<SecureSpoolEntryLease>,
    ) {
        let runtime = Arc::clone(self);
        self.workers.spawn(async move {
            let _request_lease = request_lease;
            if let Some(request_event) = request_event {
                loop {
                    #[cfg(test)]
                    let publication = tokio::select! {
                        _ = runtime.test_worker_cancel.cancelled() => return,
                        publication = runtime.publish_durable_event(request_event.clone()) => publication,
                    };
                    #[cfg(not(test))]
                    let publication = runtime.publish_durable_event(request_event.clone()).await;
                    match publication {
                        Ok(()) => break,
                        Err(error) if error.is_retryable_durable_publication() => {
                            #[cfg(test)]
                            tokio::select! {
                                _ = runtime.test_worker_cancel.cancelled() => return,
                                _ = tokio::time::sleep(RESOLUTION_OUTBOX_RETRY_DELAY) => {},
                            }
                            #[cfg(not(test))]
                            tokio::time::sleep(RESOLUTION_OUTBOX_RETRY_DELAY).await;
                        }
                        Err(error) => {
                            loop {
                                let request_dead_lettered =
                                    dead_letter_pending_request(&runtime, &request_event_id).await;
                                let rejected = dead_letter_outbox_until_committed(
                                    &path,
                                    "user-input resolution paired with a rejected request",
                                )
                                .await;
                                if request_dead_lettered {
                                    if let Some(rejected) = rejected {
                                        tracing::error!(%error, event_id = %request_event.id, path = %rejected.display(), "dead-lettered paired user-input events after permanent request rejection");
                                        break;
                                    }
                                }
                                if !wait_resolution_recovery_delay(&runtime).await {
                                    return;
                                }
                            }
                            return;
                        }
                    }
                }
            }
            loop {
                match runtime.publish_durable_event(event.clone()).await {
                    Ok(()) => {
                        if !retire_acknowledged_resolution(
                            &runtime,
                            &request_event_id,
                            &path,
                        )
                        .await
                        {
                            runtime.retry_acknowledged_resolution_retirement(
                                request_event_id.clone(),
                                path.clone(),
                            );
                        }
                        break;
                    }
                    Err(error) if error.is_retryable_durable_publication() => {
                        tracing::warn!(%error, event_id = %event.id, "retrying recovered user-input resolution");
                        tokio::time::sleep(RESOLUTION_OUTBOX_RETRY_DELAY).await;
                    }
                    Err(error) => {
                        loop {
                            let request_dead_lettered =
                                dead_letter_pending_request(&runtime, &request_event_id).await;
                            let rejected = dead_letter_outbox_until_committed(
                                &path,
                                "user-input resolution",
                            )
                            .await;
                            if request_dead_lettered {
                                if let Some(rejected) = rejected {
                                    tracing::error!(%error, event_id = %event.id, path = %rejected.display(), "dead-lettered permanently rejected user-input resolution");
                                    break;
                                }
                            }
                            if !wait_resolution_recovery_delay(&runtime).await {
                                return;
                            }
                        }
                        break;
                    }
                }
            }
        });
    }

    fn spawn_orphaned_request_cancellation(
        self: &Arc<Self>,
        request_event: nostr::Event,
        request_path: PathBuf,
        channel_id: Uuid,
        owner_pubkey: String,
        request_lease: SecureSpoolEntryLease,
    ) {
        let runtime = Arc::clone(self);
        self.workers.spawn(async move {
            loop {
                #[cfg(test)]
                let publication = tokio::select! {
                    _ = runtime.test_worker_cancel.cancelled() => return,
                    publication = runtime.publish_durable_event(request_event.clone()) => publication,
                };
                #[cfg(not(test))]
                let publication = runtime.publish_durable_event(request_event.clone()).await;
                match publication {
                    Ok(()) => break,
                    Err(error) if error.is_retryable_durable_publication() => {
                        #[cfg(test)]
                        tokio::select! {
                            _ = runtime.test_worker_cancel.cancelled() => return,
                            _ = tokio::time::sleep(RESOLUTION_OUTBOX_RETRY_DELAY) => {},
                        }
                        #[cfg(not(test))]
                        tokio::time::sleep(RESOLUTION_OUTBOX_RETRY_DELAY).await;
                    }
                    Err(error) => {
                        loop {
                            if let Some(rejected) = dead_letter_outbox_until_committed(
                                &request_path,
                                "orphaned user-input request",
                            )
                            .await
                            {
                                tracing::error!(%error, request_event_id = %request_event.id, path = %rejected.display(), "dead-lettered orphaned user-input request rejected during replay");
                                break;
                            }
                            if !wait_resolution_recovery_delay(&runtime).await {
                                return;
                            }
                        }
                        return;
                    }
                }
            }
            let request_event_id = request_event.id.to_hex();
            let resolution = match runtime.build_resolution_event(
                channel_id,
                &request_event_id,
                &owner_pubkey,
                UserInputResolutionOutcome::Cancelled,
            ) {
                Ok(event) => event,
                Err(error) => {
                    tracing::error!(%error, request_event_id, "failed to sign orphaned user-input cancellation");
                    return;
                }
            };
            let resolution_path = loop {
                let result = async {
                    let directory = resolution_outbox_dir(&runtime)?;
                    let _guard = resolution_outbox_lock().lock().await;
                    persist_outbox_event(&directory, &resolution).await
                }
                .await;
                match result {
                    Ok(path) => break path,
                    Err(error) => {
                        tracing::error!(%error, request_event_id, "failed to persist orphaned user-input cancellation; retaining recovery ownership");
                        if !wait_resolution_recovery_delay(&runtime).await {
                            return;
                        }
                    }
                }
            };
            runtime.spawn_recovered_resolution(
                resolution,
                resolution_path,
                request_event_id,
                None,
                Some(request_lease),
            );
        });
    }

    async fn publish_durable_event(
        &self,
        event: nostr::Event,
    ) -> Result<(), crate::relay::RelayError> {
        // Unit tests use the in-memory publisher pair so they can inspect the
        // exact signed event without standing up an HTTP bridge. Production
        // always requires the relay's explicit `{event_id, accepted}` ACK.
        #[cfg(test)]
        if self.rest_client.base_url == "http://127.0.0.1:0" {
            return self.test_publisher.publish_event(event).await;
        }
        self.rest_client.submit_event_accepted(&event).await
    }

    async fn admit_request_exact(
        &self,
        event: nostr::Event,
    ) -> Result<(), crate::relay::RelayError> {
        loop {
            match self.publish_durable_event(event.clone()).await {
                Ok(()) => return Ok(()),
                Err(error) if error.is_retryable_durable_publication() => {
                    tracing::warn!(
                        request_event_id = %event.id,
                        error = %error,
                        "durable user-input request admission is ambiguous; retrying the exact signed event"
                    );
                    tokio::time::sleep(REQUEST_ADMISSION_RETRY_DELAY).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn publish(
        self: &Arc<Self>,
        channel_id: Uuid,
        thread_ref: &buzz_sdk::ThreadRef,
        session_id: &str,
        turn_id: &str,
        engine: Engine,
        form: NormalizedForm,
        request_id: &str,
        message: Option<&str>,
        tool_call_id: Option<&str>,
    ) -> Result<
        (
            String,
            oneshot::Receiver<Result<Option<UserInputAnswers>, String>>,
        ),
        String,
    > {
        let request = UserInputRequest {
            request_id: request_id.to_string(),
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            channel_id: channel_id.to_string(),
            tool_call_id: tool_call_id.map(str::to_owned),
            engine,
            message: message.map(str::to_owned),
            questions: form.questions,
        };
        let content = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        let owner_pubkey = self
            .owner_cache
            .get()
            .ok_or_else(|| "agent owner is required for durable user input".to_string())?;
        let builder = buzz_sdk::build_agent_user_input_request(
            channel_id,
            thread_ref,
            owner_pubkey,
            &content,
        )
        .map_err(|e| e.to_string())?;
        let event = builder
            .sign_with_keys(&self.keys)
            .map_err(|e| e.to_string())?;
        let event_id = event.id.to_hex();
        let pending_lease = acquire_pending_request_lease(self, &event_id, true).await?;
        let _pending_request_path = match async {
            let directory = pending_request_outbox_dir(self)?;
            let _guard = resolution_outbox_lock().lock().await;
            persist_outbox_event(&directory, &event).await
        }
        .await
        {
            Ok(path) => path,
            Err(error) => {
                drop(pending_lease);
                if let Err(cleanup_error) = retire_pending_request(self, &event_id).await {
                    tracing::error!(%cleanup_error, request_event_id = %event_id, "failed to retire incomplete pending request under the root recovery lock");
                    self.retry_incomplete_pending_request_retirement(event_id.clone());
                }
                return Err(error);
            }
        };
        let (tx, rx) = oneshot::channel();
        let (admission_tx, _admission_rx) = watch::channel(false);
        self.pending.lock().await.insert(
            event_id.clone(),
            PendingRequest {
                channel_id,
                intended_owner_pubkey: owner_pubkey.to_owned(),
                sender: tx,
                admission_tx,
                resolution_started: false,
                _lease: Some(pending_lease),
            },
        );
        match tokio::time::timeout(
            REQUEST_ADMISSION_INLINE_TIMEOUT,
            self.admit_request_exact(event.clone()),
        )
        .await
        {
            Ok(Ok(())) => {
                if let Some(request) = self.pending.lock().await.get(&event_id) {
                    request.admission_tx.send_replace(true);
                }
            }
            Ok(Err(error)) => {
                if let Some(pending) = self.pending.lock().await.remove(&event_id) {
                    let _ = pending.sender.send(Err(error.to_string()));
                }
                if !dead_letter_pending_request(self, &event_id).await {
                    self.retry_pending_dead_letter(event_id.clone());
                }
                return Err(error.to_string());
            }
            Err(_) => {
                let runtime = Arc::clone(self);
                let background_event_id = event_id.clone();
                self.workers.spawn(async move {
                    tokio::time::sleep(REQUEST_ADMISSION_RETRY_DELAY).await;
                    match runtime.admit_request_exact(event).await {
                        Ok(()) => {
                            if let Some(request) =
                                runtime.pending.lock().await.get(&background_event_id)
                            {
                                request.admission_tx.send_replace(true);
                            }
                        }
                        Err(error) => {
                            if let Some(pending) =
                                runtime.pending.lock().await.remove(&background_event_id)
                            {
                                let _ = pending.sender.send(Err(error.to_string()));
                            }
                            if !dead_letter_pending_request(&runtime, &background_event_id).await {
                                runtime.retry_pending_dead_letter(background_event_id.clone());
                            }
                        }
                    }
                });
            }
        }
        Ok((event_id, rx))
    }

    pub(crate) async fn cancel(self: &Arc<Self>, event_id: &str) {
        let authority = self
            .pending
            .lock()
            .await
            .get(event_id)
            .map(|pending| (pending.channel_id, pending.intended_owner_pubkey.clone()));
        let Some((channel_id, intended_owner_pubkey)) = authority else {
            return;
        };
        if let Err(error) = self
            .start_resolution(
                channel_id,
                event_id,
                &intended_owner_pubkey,
                UserInputResolutionOutcome::Cancelled,
                None,
                &RESOLUTION_RETRY_DELAYS,
            )
            .await
        {
            tracing::warn!(%error, request_event_id = event_id, "failed to durably cancel user-input request");
        }
    }

    /// Resolve every request still owned by this runtime during graceful shutdown.
    pub(crate) async fn shutdown_pending(self: &Arc<Self>) -> bool {
        self.workers.close();
        tokio::time::timeout(RESOLUTION_SHUTDOWN_TIMEOUT, async {
            let event_ids = self
                .pending
                .lock()
                .await
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            for event_id in event_ids {
                self.cancel(&event_id).await;
            }
            self.workers.wait().await;
        })
        .await
        .is_ok()
    }

    #[allow(clippy::too_many_arguments)]
    async fn start_resolution(
        self: &Arc<Self>,
        channel_id: Uuid,
        request_event_id: &str,
        intended_owner_pubkey: &str,
        outcome: UserInputResolutionOutcome,
        completion: Option<UserInputAnswers>,
        retry_delays: &[Duration],
    ) -> Result<(), String> {
        let event = self.build_resolution_event(
            channel_id,
            request_event_id,
            intended_owner_pubkey,
            outcome,
        )?;
        let outbox_dir = resolution_outbox_dir(self)?;
        // Keep the per-request claim mutex held through durable persistence.
        // A competing answer/cancel therefore waits. If persistence fails, it
        // observes the rollback and can claim instead of being silently lost.
        let mut pending = self.pending.lock().await;
        let Some(request) = pending.get_mut(request_event_id) else {
            return Ok(());
        };
        if request.resolution_started {
            return Ok(());
        }
        request.resolution_started = true;
        let mut admission_rx = request.admission_tx.subscribe();
        let outbox_path = {
            let _guard = resolution_outbox_lock().lock().await;
            match persist_outbox_event(&outbox_dir, &event).await {
                Ok(path) => path,
                Err(error) => {
                    request.resolution_started = false;
                    return Err(error);
                }
            }
        };
        drop(pending);

        let runtime = Arc::clone(self);
        let request_event_id = request_event_id.to_owned();
        let retry_delays = retry_delays.to_vec();
        self.workers.spawn(async move {
            while !*admission_rx.borrow() {
                #[cfg(test)]
                tokio::select! {
                    _ = runtime.test_worker_cancel.cancelled() => return,
                    changed = admission_rx.changed() => {
                        if changed.is_err() {
                            return;
                        }
                    }
                }
                #[cfg(not(test))]
                if admission_rx.changed().await.is_err() {
                    return;
                }
            }
            let mut retry_index = 0_usize;
            loop {
                match runtime.publish_durable_event(event.clone()).await {
                    Ok(()) => {
                        if !retire_acknowledged_resolution(
                            &runtime,
                            &request_event_id,
                            &outbox_path,
                        )
                        .await
                        {
                            runtime.retry_acknowledged_resolution_retirement(
                                request_event_id.clone(),
                                outbox_path.clone(),
                            );
                        }
                        if let Some(pending) =
                            runtime.pending.lock().await.remove(&request_event_id)
                        {
                            let _ = pending.sender.send(Ok(completion));
                        }
                        break;
                    }
                    Err(error) if error.is_retryable_durable_publication() => {
                        tracing::warn!(%error, request_event_id, "retrying durable user-input resolution");
                        let delay = retry_delays
                            .get(retry_index)
                            .copied()
                            .or_else(|| retry_delays.last().copied())
                            .unwrap_or(Duration::from_secs(1));
                        retry_index = retry_index.saturating_add(1);
                        tokio::time::sleep(delay).await;
                    }
                    Err(error) => {
                        if let Some(pending) =
                            runtime.pending.lock().await.remove(&request_event_id)
                        {
                            let _ = pending.sender.send(Err(error.to_string()));
                        }
                        loop {
                            let request_dead_lettered =
                                dead_letter_pending_request(&runtime, &request_event_id).await;
                            let rejected = dead_letter_outbox_until_committed(
                                &outbox_path,
                                "user-input resolution",
                            )
                            .await;
                            if request_dead_lettered {
                                if let Some(rejected) = rejected {
                                    tracing::error!(%error, request_event_id, path = %rejected.display(), "dead-lettered permanently rejected user-input resolution");
                                    break;
                                }
                            }
                            if !wait_resolution_recovery_delay(&runtime).await {
                                return;
                            }
                        }
                        break;
                    }
                }
            }
        });
        Ok(())
    }

    fn build_resolution_event(
        &self,
        channel_id: Uuid,
        request_event_id: &str,
        intended_owner_pubkey: &str,
        outcome: UserInputResolutionOutcome,
    ) -> Result<nostr::Event, String> {
        let content = serde_json::to_string(&UserInputResolved {
            request_event_id: request_event_id.to_owned(),
            outcome,
        })
        .map_err(|error| error.to_string())?;
        let builder = buzz_sdk::build_agent_user_input_resolved(
            channel_id,
            request_event_id,
            intended_owner_pubkey,
            &content,
        )
        .map_err(|error| error.to_string())?;
        builder
            .sign_with_keys(&self.keys)
            .map_err(|error| error.to_string())
    }

    #[cfg(test)]
    async fn publish_resolution_with_retry_delays(
        &self,
        channel_id: Uuid,
        request_event_id: &str,
        intended_owner_pubkey: &str,
        outcome: UserInputResolutionOutcome,
        retry_delays: &[Duration],
    ) -> Result<(), String> {
        let event = self.build_resolution_event(
            channel_id,
            request_event_id,
            intended_owner_pubkey,
            outcome,
        )?;
        let mut retry_index = 0_usize;
        loop {
            match self.publish_durable_event(event.clone()).await {
                Ok(()) => return Ok(()),
                Err(error) if !error.is_retryable_durable_publication() => {
                    return Err(error.to_string());
                }
                Err(error) => {
                    let Some(delay) = retry_delays.get(retry_index).copied() else {
                        return Err(error.to_string());
                    };
                    retry_index = retry_index.saturating_add(1);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    pub(crate) async fn handle_event(self: &Arc<Self>, buzz_event: &BuzzEvent) {
        if buzz_event.event.kind.as_u16() as u32 != KIND_AGENT_USER_INPUT_ANSWER {
            return;
        }
        let Some(request_event_id) = single_relationship_tag(&buzz_event.event, "e") else {
            tracing::warn!("ignoring user-input answer without exactly one request relationship");
            return;
        };
        let pending_authority = self
            .pending
            .lock()
            .await
            .get(request_event_id)
            .map(|pending| (pending.channel_id, pending.intended_owner_pubkey.clone()));
        let Some((pending_channel_id, intended_owner_pubkey)) = pending_authority else {
            tracing::debug!(request_event_id, "ignoring late user-input answer");
            return;
        };
        let declared_channel = single_relationship_tag(&buzz_event.event, "h");
        if buzz_event.channel_id != pending_channel_id
            || declared_channel != Some(pending_channel_id.to_string().as_str())
        {
            tracing::warn!(request_event_id, "ignoring cross-channel user-input answer");
            return;
        }
        let requesting_agent_pubkey = self.keys.public_key().to_hex();
        if single_relationship_tag(&buzz_event.event, "p") != Some(requesting_agent_pubkey.as_str())
        {
            tracing::warn!(
                request_event_id,
                "ignoring user-input answer with the wrong requesting-agent relationship"
            );
            return;
        }
        if !answer_author_is_intended_owner(
            &buzz_event.event.pubkey.to_hex(),
            &intended_owner_pubkey,
        ) {
            tracing::warn!(
                author = %buzz_event.event.pubkey,
                request_event_id,
                "ignoring non-owner user-input answer"
            );
            return;
        }
        let answers = match serde_json::from_str::<UserInputAnswers>(&buzz_event.event.content) {
            Ok(answers) => answers,
            Err(error) => {
                tracing::warn!(%error, "ignoring malformed user-input answer");
                return;
            }
        };
        let declined = answers.values().all(Option::is_none);
        if let Err(error) = self
            .start_resolution(
                pending_channel_id,
                request_event_id,
                &intended_owner_pubkey,
                if declined {
                    UserInputResolutionOutcome::Declined
                } else {
                    UserInputResolutionOutcome::Answered
                },
                Some(answers),
                &RESOLUTION_RETRY_DELAYS,
            )
            .await
        {
            tracing::warn!(%error, request_event_id, "failed to durably resolve user-input request");
        }
    }
}

fn option(value: &serde_json::Value) -> Option<Option_> {
    let object = value.as_object()?;
    let value = object.get("const")?.as_str()?.to_owned();
    Some(Option_ {
        value: value.clone(),
        label: object
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&value)
            .to_owned(),
        description: object
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

fn natural_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    let mut l = left.chars().peekable();
    let mut r = right.chars().peekable();
    loop {
        match (l.peek(), r.peek()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(a), Some(b)) if a.is_ascii_digit() && b.is_ascii_digit() => {
                let mut ln = String::new();
                let mut rn = String::new();
                while l.peek().is_some_and(char::is_ascii_digit) {
                    ln.push(l.next().unwrap_or_default());
                }
                while r.peek().is_some_and(char::is_ascii_digit) {
                    rn.push(r.next().unwrap_or_default());
                }
                let ordering = ln
                    .parse::<u64>()
                    .unwrap_or(u64::MAX)
                    .cmp(&rn.parse::<u64>().unwrap_or(u64::MAX));
                if ordering != std::cmp::Ordering::Equal {
                    return ordering;
                }
            }
            (Some(a), Some(b)) => {
                let ordering = a.cmp(b);
                l.next();
                r.next();
                if ordering != std::cmp::Ordering::Equal {
                    return ordering;
                }
            }
        }
    }
}

/// Normalize the supported ACP form subset into Crew's contract.
pub(crate) fn normalize_form(schema: &serde_json::Value) -> Option<NormalizedForm> {
    let properties = schema.get("properties")?.as_object()?;
    let required = schema
        .get("required")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let mut questions = Vec::new();
    let mut mappings = Vec::new();

    let mut property_keys = properties.keys().collect::<Vec<_>>();
    property_keys.sort_by(|left, right| natural_cmp(left, right));
    for key in property_keys {
        let field = properties.get(key)?;
        if key.ends_with("_custom") {
            continue;
        }
        let index = questions.len();
        let object = field.as_object()?;
        let (options, multi_select) = if let Some(values) = object.get("oneOf") {
            let values = values.as_array()?;
            (
                values.iter().map(option).collect::<Option<Vec<_>>>()?,
                false,
            )
        } else if let Some(items) = object.get("items") {
            let values = items.get("anyOf")?.as_array()?;
            (values.iter().map(option).collect::<Option<Vec<_>>>()?, true)
        } else if object.get("type").and_then(serde_json::Value::as_str) == Some("string") {
            (Vec::new(), false)
        } else {
            return None;
        };
        let id = format!("q{index}");
        let custom_key = properties
            .contains_key(&format!("{key}_custom"))
            .then(|| format!("{key}_custom"));
        let question = UserInputQuestion {
            id: id.clone(),
            header: object
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(key)
                .to_owned(),
            question: object
                .get("description")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(key)
                .to_owned(),
            options,
            multi_select,
            allow_custom_answer: custom_key.is_some(),
            required: required.contains(key.as_str()),
            // ACP has no notes concept; intentionally false until an engine
            // provides a notes affordance.
            allow_notes: false,
        };
        questions.push(question);
        mappings.push(FieldMapping {
            id,
            field_key: key.clone(),
            custom_key,
            multi_select,
            required: required.contains(key.as_str()),
        });
    }
    (!questions.is_empty()).then_some(NormalizedForm {
        questions,
        mappings,
    })
}

/// Rebuild ACP content using native field keys.
pub(crate) fn reconstruct_content(
    form: &NormalizedForm,
    answers: &BTreeMap<String, Option<UserInputAnswer>>,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut content = serde_json::Map::new();
    for mapping in &form.mappings {
        let question = form
            .questions
            .iter()
            .find(|question| question.id == mapping.id)?;
        let wire_value = |value: String| {
            question
                .options
                .iter()
                .find(|option| option.label == value || option.value == value)
                .map(|option| option.value.clone())
                .unwrap_or(value)
        };
        let answer = match answers.get(&mapping.id) {
            Some(Some(UserInputAnswer::Skipped)) | Some(None) | None if !mapping.required => {
                continue
            }
            Some(Some(UserInputAnswer::Skipped)) | Some(None) | None => return None,
            Some(Some(answer)) => answer.clone(),
        };
        let (key, value) = match answer {
            UserInputAnswer::Text(value) => {
                let matches_option = question
                    .options
                    .iter()
                    .any(|option| option.value == value || option.label == value);
                if matches_option {
                    (
                        mapping.field_key.clone(),
                        serde_json::Value::String(wire_value(value)),
                    )
                } else if let Some(custom_key) = &mapping.custom_key {
                    (custom_key.clone(), serde_json::Value::String(value))
                } else {
                    (
                        mapping.field_key.clone(),
                        serde_json::Value::String(wire_value(value)),
                    )
                }
            }
            UserInputAnswer::Multi(values) => (
                mapping.field_key.clone(),
                serde_json::Value::Array(
                    values
                        .into_iter()
                        .map(wire_value)
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            ),
            UserInputAnswer::Structured {
                selected,
                choice_notes,
            } => {
                let selected = match selected {
                    UserInputSelection::One(value) => vec![value],
                    UserInputSelection::Many(values) => values,
                };
                let selected = selected.into_iter().map(wire_value).collect::<Vec<_>>();
                let value = if mapping.multi_select {
                    serde_json::Value::Array(
                        selected
                            .into_iter()
                            .map(serde_json::Value::String)
                            .collect(),
                    )
                } else {
                    serde_json::Value::String(selected.into_iter().next()?)
                };
                if let Some(custom_key) = &mapping.custom_key {
                    if !choice_notes.is_empty() {
                        (
                            custom_key.clone(),
                            serde_json::Value::String(
                                choice_notes.values().next().cloned().unwrap_or_default(),
                            ),
                        )
                    } else {
                        (mapping.field_key.clone(), value)
                    }
                } else {
                    (mapping.field_key.clone(), value)
                }
            }
            UserInputAnswer::Skipped => return None,
        };
        content.insert(key, value);
    }
    Some(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::mpsc;

    fn test_thread_ref() -> buzz_sdk::ThreadRef {
        let event_id = nostr::EventId::from_hex(&"a".repeat(64)).expect("test event id");
        buzz_sdk::ThreadRef {
            root_event_id: event_id,
            parent_event_id: event_id,
        }
    }

    async fn rejected_admission_server() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind admission server");
        let base_url = format!("http://{}", listener.local_addr().expect("server address"));
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept event submission");
            let mut request = Vec::new();
            let mut chunk = [0_u8; 4096];
            let (body_start, content_length) = loop {
                let read = socket.read(&mut chunk).await.expect("read submission");
                assert!(read > 0, "submission ended before HTTP headers");
                request.extend_from_slice(&chunk[..read]);
                let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = std::str::from_utf8(&request[..header_end]).expect("HTTP headers");
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .expect("content length");
                break (header_end + 4, content_length);
            };
            while request.len() < body_start + content_length {
                let read = socket.read(&mut chunk).await.expect("read submission body");
                assert!(read > 0, "submission body ended early");
                request.extend_from_slice(&chunk[..read]);
            }
            let event: nostr::Event =
                serde_json::from_slice(&request[body_start..body_start + content_length])
                    .expect("signed event body");
            let body = serde_json::json!({
                "event_id": event.id.to_hex(),
                "accepted": false,
                "message": "policy rejected",
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write rejected ACK");
        });
        base_url
    }

    async fn sequenced_admission_server(
        admissions: Vec<bool>,
    ) -> (String, tokio::task::JoinHandle<Vec<nostr::EventId>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind sequenced admission server");
        let base_url = format!("http://{}", listener.local_addr().expect("server address"));
        let server = tokio::spawn(async move {
            let mut event_ids = Vec::new();
            for accepted in admissions {
                let (mut socket, _) = listener.accept().await.expect("accept event submission");
                let mut request = Vec::new();
                let mut chunk = [0_u8; 4096];
                let (body_start, content_length) = loop {
                    let read = socket.read(&mut chunk).await.expect("read submission");
                    assert!(read > 0, "submission ended before HTTP headers");
                    request.extend_from_slice(&chunk[..read]);
                    let Some(header_end) =
                        request.windows(4).position(|window| window == b"\r\n\r\n")
                    else {
                        continue;
                    };
                    let headers =
                        std::str::from_utf8(&request[..header_end]).expect("HTTP headers");
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .expect("content length");
                    break (header_end + 4, content_length);
                };
                while request.len() < body_start + content_length {
                    let read = socket.read(&mut chunk).await.expect("read submission body");
                    assert!(read > 0, "submission body ended early");
                    request.extend_from_slice(&chunk[..read]);
                }
                let event: nostr::Event =
                    serde_json::from_slice(&request[body_start..body_start + content_length])
                        .expect("signed event body");
                event_ids.push(event.id);
                let (status, body) = if accepted {
                    (
                        "200 OK",
                        serde_json::json!({
                            "event_id": event.id.to_hex(),
                            "accepted": true,
                            "message": "stored",
                        })
                        .to_string(),
                    )
                } else {
                    ("503 Service Unavailable", "{}".to_string())
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write admission ACK");
            }
            event_ids
        });
        (base_url, server)
    }

    #[test]
    fn user_input_answer_requires_the_intended_owner() {
        assert!(answer_author_is_intended_owner("owner", "owner"));
        assert!(!answer_author_is_intended_owner(
            "same-owner-sibling",
            "owner"
        ));
    }

    #[tokio::test]
    async fn request_is_not_pending_until_relay_returns_exact_accepted_ack() {
        let owner = Keys::generate();
        let agent = Keys::generate();
        let (publisher, _published) = RelayEventPublisher::test_pair();
        let runtime = QuestionRuntime::new(
            publisher,
            agent.clone(),
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            RestClient {
                http: reqwest::Client::new(),
                base_url: rejected_admission_server().await,
                keys: agent,
                auth_tag_json: None,
            },
        );
        let form = normalize_form(&serde_json::json!({
            "type": "object",
            "properties": {"question_0": {"type": "string"}}
        }))
        .expect("supported form");

        let error = runtime
            .publish(
                Uuid::new_v4(),
                &test_thread_ref(),
                "session",
                "turn",
                Engine::Codex,
                form,
                "request",
                Some("Need input"),
                None,
            )
            .await
            .expect_err("accepted=false must fail durable request publication");
        assert!(error.contains("relay rejected event"));
        assert!(runtime.pending.lock().await.is_empty());
    }

    #[tokio::test]
    async fn ambiguous_request_admission_hands_off_exact_event_without_cancelling() {
        let owner = Keys::generate();
        let agent = Keys::generate();
        let (base_url, server) =
            sequenced_admission_server(vec![false, false, false, false, true]).await;
        let (publisher, _published) = RelayEventPublisher::test_pair();
        let runtime = QuestionRuntime::new(
            publisher,
            agent.clone(),
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            RestClient {
                http: reqwest::Client::new(),
                base_url,
                keys: agent,
                auth_tag_json: None,
            },
        );
        let form = normalize_form(&serde_json::json!({
            "type": "object",
            "properties": {"question_0": {"type": "string"}}
        }))
        .expect("supported form");

        let (request_event_id, receiver) = runtime
            .publish(
                Uuid::new_v4(),
                &test_thread_ref(),
                "session",
                "turn-ambiguous",
                Engine::Codex,
                form,
                "request",
                Some("Need input"),
                None,
            )
            .await
            .expect("ambiguous admission transfers to a background owner");
        assert!(
            !*runtime
                .pending
                .lock()
                .await
                .get(&request_event_id)
                .expect("pending request")
                .admission_tx
                .borrow(),
            "relay dispatch is released before the exact accepted ACK"
        );
        let submitted_ids = server.await.expect("transient server completes");
        assert!(submitted_ids.windows(2).all(|ids| ids[0] == ids[1]));
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if runtime
                    .pending
                    .lock()
                    .await
                    .get(&request_event_id)
                    .is_some_and(|pending| *pending.admission_tx.borrow())
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("background owner records the exact accepted ACK");
        let outbox_dir = pending_request_outbox_dir(&runtime).expect("request outbox");
        let persisted = std::fs::read_dir(&outbox_dir)
            .expect("read request outbox")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("json")
            })
            .count();
        assert_eq!(persisted, 1, "ambiguous ACK must retain request authority");
        assert_eq!(runtime.pending.lock().await.len(), 1);
        drop(receiver);
        runtime.stop_test_workers().await;
        let _ = std::fs::remove_dir_all(outbox_dir);
    }

    #[tokio::test]
    async fn resolution_retries_the_same_signed_event_until_exact_ack() {
        let owner = Keys::generate();
        let agent = Keys::generate();
        let request = nostr::EventBuilder::text_note("request")
            .sign_with_keys(&agent)
            .expect("sign request");
        let (base_url, server) = sequenced_admission_server(vec![false, true]).await;
        let (publisher, _published) = RelayEventPublisher::test_pair();
        let runtime = QuestionRuntime::new(
            publisher,
            agent.clone(),
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            RestClient {
                http: reqwest::Client::new(),
                base_url,
                keys: agent,
                auth_tag_json: None,
            },
        );

        runtime
            .publish_resolution_with_retry_delays(
                Uuid::new_v4(),
                &request.id.to_hex(),
                &owner.public_key().to_hex(),
                UserInputResolutionOutcome::Answered,
                &[std::time::Duration::from_millis(1)],
            )
            .await
            .expect("second admission accepts resolution");

        let event_ids = server.await.expect("admission server completes");
        assert_eq!(event_ids.len(), 2);
        assert_eq!(event_ids[0], event_ids[1]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn secure_spool_rejects_an_existing_non_owner_only_directory() {
        use std::os::unix::fs::PermissionsExt;

        let directory = std::fs::canonicalize(std::env::temp_dir())
            .unwrap_or_else(|_| std::env::temp_dir())
            .join(format!("buzz-acp-insecure-spool-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&directory)
            .await
            .expect("create insecure spool");
        tokio::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755))
            .await
            .expect("make spool group-readable");

        let result = ensure_secure_spool_dir(&directory).await;

        assert!(result.is_err(), "unsafe existing spool must fail closed");
        let _ = tokio::fs::remove_dir_all(directory).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn spool_capacity_rejects_symlinked_entries() {
        let directory = std::fs::canonicalize(std::env::temp_dir())
            .unwrap_or_else(|_| std::env::temp_dir())
            .join(format!("buzz-acp-symlink-spool-{}", Uuid::new_v4()));
        ensure_secure_spool_dir(&directory)
            .await
            .expect("create secure spool");
        let target = directory.with_extension("target");
        tokio::fs::write(&target, b"not an outbox entry")
            .await
            .expect("write symlink target");
        std::os::unix::fs::symlink(&target, directory.join("entry.json"))
            .expect("create symlinked entry");

        let result = ensure_spool_capacity(&directory, 1, 1).await;

        assert!(result.is_err(), "capacity scan must not follow symlinks");
        let _ = tokio::fs::remove_dir_all(&directory).await;
        let _ = tokio::fs::remove_file(target).await;
    }

    #[tokio::test]
    async fn resolution_outbox_retries_until_ack_before_releasing_the_answer() {
        let owner = Keys::generate();
        let agent = Keys::generate();
        let request = nostr::EventBuilder::text_note("request")
            .sign_with_keys(&agent)
            .expect("sign request");
        let request_event_id = request.id.to_hex();
        let (base_url, server) = sequenced_admission_server(vec![false, false, true]).await;
        let (publisher, _published) = RelayEventPublisher::test_pair();
        let runtime = QuestionRuntime::new(
            publisher,
            agent.clone(),
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            RestClient {
                http: reqwest::Client::new(),
                base_url,
                keys: agent,
                auth_tag_json: None,
            },
        );
        let (sender, receiver) = oneshot::channel();
        let (admission_tx, _admission_rx) = watch::channel(true);
        runtime.pending.lock().await.insert(
            request_event_id.clone(),
            PendingRequest {
                channel_id: Uuid::new_v4(),
                intended_owner_pubkey: owner.public_key().to_hex(),
                sender,
                admission_tx,
                resolution_started: false,
                _lease: None,
            },
        );
        let completion = BTreeMap::from([(
            "q0".to_string(),
            Some(UserInputAnswer::Text("answer".to_string())),
        )]);

        runtime
            .start_resolution(
                Uuid::new_v4(),
                &request_event_id,
                &owner.public_key().to_hex(),
                UserInputResolutionOutcome::Answered,
                Some(completion.clone()),
                &[Duration::from_millis(1)],
            )
            .await
            .expect("enqueue resolution");

        let delivered = tokio::time::timeout(Duration::from_secs(3), receiver)
            .await
            .expect("outbox eventually accepts")
            .expect("completion sender remains live");
        assert_eq!(delivered, Ok(Some(completion)));
        let event_ids = server.await.expect("admission server completes");
        assert_eq!(event_ids.len(), 3);
        assert!(event_ids.iter().all(|event_id| *event_id == event_ids[0]));
    }

    #[tokio::test]
    async fn pre_ack_answer_is_durably_claimed_without_blocking_relay_dispatch() {
        let owner = Keys::generate();
        let agent = Keys::generate();
        let request = nostr::EventBuilder::text_note("request")
            .sign_with_keys(&agent)
            .expect("sign request");
        let request_event_id = request.id.to_hex();
        let (publisher, mut published) = RelayEventPublisher::test_pair();
        let runtime = QuestionRuntime::new(
            publisher,
            agent.clone(),
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            RestClient {
                http: reqwest::Client::new(),
                base_url: "http://127.0.0.1:0".to_owned(),
                keys: agent,
                auth_tag_json: None,
            },
        );
        let (sender, _receiver) = oneshot::channel();
        let (admission_tx, _admission_rx) = watch::channel(false);
        runtime.pending.lock().await.insert(
            request_event_id.clone(),
            PendingRequest {
                channel_id: Uuid::new_v4(),
                intended_owner_pubkey: owner.public_key().to_hex(),
                sender,
                admission_tx: admission_tx.clone(),
                resolution_started: false,
                _lease: None,
            },
        );

        // This boundary proves the answer handler does not wait for the
        // request-admission ACK. Leave enough wall-clock headroom for the
        // required fsync on a loaded CI runner; admission remains withheld for
        // the entire deadline, so an ACK-wait regression still times out.
        tokio::time::timeout(
            Duration::from_secs(1),
            runtime.start_resolution(
                Uuid::new_v4(),
                &request_event_id,
                &owner.public_key().to_hex(),
                UserInputResolutionOutcome::Answered,
                Some(BTreeMap::new()),
                &[Duration::from_millis(1)],
            ),
        )
        .await
        .expect("pre-ACK answer must not block relay dispatch")
        .expect("answer intent is durably claimed");
        assert!(
            runtime
                .pending
                .lock()
                .await
                .get(&request_event_id)
                .is_some_and(|pending| pending.resolution_started),
            "answer must own the terminal transition before admission ACK"
        );

        admission_tx.send_replace(true);
        tokio::time::timeout(Duration::from_secs(1), published.recv())
            .await
            .expect("resolution publishes after admission")
            .expect("resolution event");
        runtime.stop_test_workers().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn pending_request_retirement_serializes_with_root_wide_recovery() {
        let owner = Keys::generate();
        let agent = Keys::generate();
        let (publisher, _published) = RelayEventPublisher::test_pair();
        let runtime = QuestionRuntime::new(
            publisher,
            agent.clone(),
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            RestClient {
                http: reqwest::Client::new(),
                base_url: format!("http://retirement-lock-{}", Uuid::new_v4()),
                keys: agent,
                auth_tag_json: None,
            },
        );
        let request_event_id = "retirement-lock-request";
        let root = resolution_outbox_dir(&runtime).expect("resolution root");
        let pending = pending_request_outbox_dir(&runtime).expect("pending root");
        ensure_secure_spool_dir(&root).await.expect("secure root");
        ensure_secure_spool_dir(&pending)
            .await
            .expect("secure pending root");
        assert!(write_secure_entry_if_absent(
            &pending,
            format!("{request_event_id}.json").as_ref(),
            format!("{request_event_id}.tmp").as_ref(),
            b"{}",
        )
        .await
        .expect("persist request"));
        drop(
            lock_secure_entry_lease(
                &pending,
                pending_request_lease_name(request_event_id).as_ref(),
            )
            .await
            .expect("create lease"),
        );

        let root_lock = lock_secure_directory(&root)
            .await
            .expect("hold recovery lock");
        let retire_runtime = Arc::clone(&runtime);
        let retire =
            tokio::spawn(
                async move { retire_pending_request(&retire_runtime, request_event_id).await },
            );
        let error = retire
            .await
            .expect("join contended retirement")
            .expect_err("retirement must fail closed while recovery owns the root");
        assert_eq!(error, SECURE_SPOOL_LOCK_CONTENDED);
        runtime.retry_incomplete_pending_request_retirement(request_event_id.to_owned());
        drop(root_lock);
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let request_absent = read_secure_entry(
                    &pending,
                    format!("{request_event_id}.json").as_ref(),
                    MAX_SPOOL_BYTES,
                )
                .await
                .expect("inspect request cleanup")
                .is_none();
                let lease_absent = read_secure_entry(
                    &pending,
                    pending_request_lease_name(request_event_id).as_ref(),
                    MAX_SPOOL_BYTES,
                )
                .await
                .expect("inspect lease cleanup")
                .is_none();
                if request_absent && lease_absent {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("tracked retirement removes request and lease after lock release");

        assert!(write_secure_entry_if_absent(
            &pending,
            format!("{request_event_id}.json").as_ref(),
            format!("{request_event_id}.retry.tmp").as_ref(),
            b"{}",
        )
        .await
        .expect("repersist request"));
        drop(
            lock_secure_entry_lease(
                &pending,
                pending_request_lease_name(request_event_id).as_ref(),
            )
            .await
            .expect("recreate lease"),
        );
        let root_lock = lock_secure_directory(&root)
            .await
            .expect("hold recovery lock");
        assert!(
            !dead_letter_pending_request(&runtime, request_event_id).await,
            "dead-letter cleanup must fail closed while recovery owns the root"
        );
        drop(root_lock);
        assert!(dead_letter_pending_request(&runtime, request_event_id).await);

        runtime.stop_test_workers().await;
        std::fs::remove_dir_all(root).expect("clean retirement spool");
    }

    #[tokio::test]
    async fn restart_replays_and_cancels_an_orphaned_pending_request() {
        let owner = Keys::generate();
        let agent = Keys::generate();
        let channel_id = Uuid::new_v4();
        let rest_client = RestClient {
            http: reqwest::Client::new(),
            base_url: "http://127.0.0.1:0".to_string(),
            keys: agent.clone(),
            auth_tag_json: None,
        };
        let (publisher, mut initially_published) = RelayEventPublisher::test_pair();
        let runtime = QuestionRuntime::new(
            publisher,
            agent.clone(),
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            rest_client.clone(),
        );
        let _ = tokio::fs::remove_dir_all(pending_request_outbox_dir(&runtime).unwrap()).await;
        let _ = tokio::fs::remove_dir_all(resolution_outbox_dir(&runtime).unwrap()).await;
        let form = normalize_form(&serde_json::json!({
            "type": "object",
            "properties": {"question_0": {"type": "string"}}
        }))
        .expect("supported form");

        let (request_event_id, receiver) = runtime
            .publish(
                channel_id,
                &test_thread_ref(),
                "session",
                "turn",
                Engine::Codex,
                form,
                "request",
                Some("Need input"),
                None,
            )
            .await
            .expect("publish request");
        let original = initially_published.recv().await.expect("request event");
        assert_eq!(original.id.to_hex(), request_event_id);
        drop(receiver);
        drop(runtime);

        let (publisher, mut recovered) = RelayEventPublisher::test_pair();
        let restarted = QuestionRuntime::new(
            publisher,
            agent,
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            rest_client,
        );
        restarted.resume_resolution_outbox().await;

        let replayed = tokio::time::timeout(Duration::from_secs(1), recovered.recv())
            .await
            .expect("orphan recovery starts")
            .expect("replayed request");
        assert_eq!(replayed.id, original.id);
        let resolved = tokio::time::timeout(Duration::from_secs(1), recovered.recv())
            .await
            .expect("orphan cancellation publishes")
            .expect("resolution event");
        let resolution: UserInputResolved =
            serde_json::from_str(&resolved.content).expect("resolution content");
        assert_eq!(resolution.request_event_id, request_event_id);
        assert_eq!(resolution.outcome, UserInputResolutionOutcome::Cancelled);
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let path = pending_request_outbox_dir(&restarted)
                    .unwrap()
                    .join(format!("{request_event_id}.json"));
                if !tokio::fs::try_exists(path).await.unwrap_or(true) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("pending request ledger retires after cancellation ACK");
    }

    #[tokio::test]
    async fn overlapping_recovery_does_not_cancel_a_live_pending_request() {
        let owner = Keys::generate();
        let agent = Keys::generate();
        let channel_id = Uuid::new_v4();
        let rest_client = RestClient {
            http: reqwest::Client::new(),
            base_url: "http://127.0.0.1:0".to_string(),
            keys: agent.clone(),
            auth_tag_json: None,
        };
        let (publisher, mut initially_published) = RelayEventPublisher::test_pair();
        let runtime = QuestionRuntime::new(
            publisher,
            agent.clone(),
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            rest_client.clone(),
        );
        let _ = tokio::fs::remove_dir_all(resolution_outbox_dir(&runtime).unwrap()).await;
        let form = normalize_form(&serde_json::json!({
            "type": "object",
            "properties": {"question_0": {"type": "string"}}
        }))
        .expect("supported form");
        let (request_event_id, _receiver) = runtime
            .publish(
                channel_id,
                &test_thread_ref(),
                "session",
                "turn",
                Engine::Codex,
                form,
                "request",
                Some("Need input"),
                None,
            )
            .await
            .expect("publish live request");
        initially_published.recv().await.expect("request event");

        let (recovery_publisher, mut recovered) = RelayEventPublisher::test_pair();
        let overlapping = QuestionRuntime::new(
            recovery_publisher,
            agent,
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            rest_client,
        );
        assert!(
            !overlapping.resume_resolution_outbox().await,
            "a contended live lease must keep tracked recovery ownership active"
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(50), recovered.recv())
                .await
                .is_err(),
            "a sibling process must not replay or cancel a request with a live lease"
        );
        assert!(runtime.pending.lock().await.contains_key(&request_event_id));
        assert!(
            read_secure_entry(
                &pending_request_outbox_dir(&runtime).unwrap(),
                format!("{request_event_id}.json").as_ref(),
                MAX_SPOOL_BYTES,
            )
            .await
            .expect("read pending entry")
            .is_some(),
            "overlapping recovery must preserve the live pending ledger"
        );

        let abandoned = runtime
            .pending
            .lock()
            .await
            .remove(&request_event_id)
            .expect("release simulated crashed owner lease");
        drop(abandoned);
        assert!(overlapping.resume_resolution_outbox().await);
        let replayed = tokio::time::timeout(Duration::from_secs(1), recovered.recv())
            .await
            .expect("surviving process rescans released lease")
            .expect("replayed abandoned request");
        assert_eq!(replayed.id.to_hex(), request_event_id);

        runtime.stop_test_workers().await;
        overlapping.stop_test_workers().await;
        let _ = tokio::fs::remove_dir_all(resolution_outbox_dir(&runtime).unwrap()).await;
    }

    #[tokio::test]
    async fn restart_replays_paired_request_before_its_terminal_resolution() {
        let owner = Keys::generate();
        let agent = Keys::generate();
        let channel_id = Uuid::new_v4();
        let rest_client = RestClient {
            http: reqwest::Client::new(),
            base_url: "http://127.0.0.1:0".to_string(),
            keys: agent.clone(),
            auth_tag_json: None,
        };
        let (publisher, mut initially_published) = RelayEventPublisher::test_pair();
        let runtime = QuestionRuntime::new(
            publisher,
            agent.clone(),
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            rest_client.clone(),
        );
        let pending_dir = pending_request_outbox_dir(&runtime).expect("pending directory");
        let resolution_dir = resolution_outbox_dir(&runtime).expect("resolution directory");
        let _ = tokio::fs::remove_dir_all(&pending_dir).await;
        let _ = tokio::fs::remove_dir_all(&resolution_dir).await;
        let form = normalize_form(&serde_json::json!({
            "type": "object",
            "properties": {"question_0": {"type": "string"}}
        }))
        .expect("supported form");
        let (request_event_id, receiver) = runtime
            .publish(
                channel_id,
                &test_thread_ref(),
                "session",
                "turn",
                Engine::Codex,
                form,
                "request",
                Some("Need input"),
                None,
            )
            .await
            .expect("publish request");
        let original = initially_published.recv().await.expect("request event");
        let resolution = runtime
            .build_resolution_event(
                channel_id,
                &request_event_id,
                &owner.public_key().to_hex(),
                UserInputResolutionOutcome::Cancelled,
            )
            .expect("resolution event");
        {
            let _guard = resolution_outbox_lock().lock().await;
            persist_outbox_event(&resolution_dir, &resolution)
                .await
                .expect("persist paired resolution");
        }
        drop(receiver);
        drop(runtime);

        let (publisher, mut recovered) = RelayEventPublisher::test_pair();
        let restarted = QuestionRuntime::new(
            publisher,
            agent,
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            rest_client,
        );
        assert!(restarted.resume_resolution_outbox().await);
        let replayed_request = tokio::time::timeout(Duration::from_secs(1), recovered.recv())
            .await
            .expect("paired request recovery starts")
            .expect("paired request event");
        let replayed_resolution = tokio::time::timeout(Duration::from_secs(1), recovered.recv())
            .await
            .expect("paired resolution follows request ACK")
            .expect("paired resolution event");
        assert_eq!(replayed_request.id, original.id);
        assert_eq!(replayed_resolution.id, resolution.id);
        restarted.stop_test_workers().await;
    }

    #[tokio::test]
    async fn malformed_resolution_is_quarantined_without_blocking_healthy_siblings() {
        let owner = Keys::generate();
        let agent = Keys::generate();
        let rest_client = RestClient {
            http: reqwest::Client::new(),
            base_url: "http://127.0.0.1:0".to_string(),
            keys: agent.clone(),
            auth_tag_json: None,
        };
        let (publisher, mut initially_published) = RelayEventPublisher::test_pair();
        let runtime = QuestionRuntime::new(
            publisher,
            agent.clone(),
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            rest_client.clone(),
        );
        let pending_dir = pending_request_outbox_dir(&runtime).expect("pending directory");
        let resolution_dir = resolution_outbox_dir(&runtime).expect("resolution directory");
        let _ = tokio::fs::remove_dir_all(&pending_dir).await;
        let _ = tokio::fs::remove_dir_all(&resolution_dir).await;
        let form = normalize_form(&serde_json::json!({
            "type": "object",
            "properties": {"question_0": {"type": "string"}}
        }))
        .expect("supported form");
        let (request_event_id, receiver) = runtime
            .publish(
                Uuid::new_v4(),
                &test_thread_ref(),
                "session",
                "turn",
                Engine::Codex,
                form,
                "request",
                Some("Need input"),
                None,
            )
            .await
            .expect("publish request");
        initially_published.recv().await.expect("request event");
        drop(receiver);
        drop(runtime);

        assert!(write_secure_entry_if_absent(
            &resolution_dir,
            std::ffi::OsStr::new("malformed.json"),
            std::ffi::OsStr::new("malformed.tmp"),
            b"{",
        )
        .await
        .expect("write malformed resolution"));
        let (publisher, mut recovered) = RelayEventPublisher::test_pair();
        let restarted = QuestionRuntime::new(
            publisher,
            agent,
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            rest_client,
        );
        assert!(restarted.resume_resolution_outbox().await);
        let replayed = tokio::time::timeout(Duration::from_secs(1), recovered.recv())
            .await
            .expect("healthy sibling request replays")
            .expect("request event");
        assert_eq!(replayed.id.to_hex(), request_event_id);
        let resolved = tokio::time::timeout(Duration::from_secs(1), recovered.recv())
            .await
            .expect("healthy sibling cancellation publishes")
            .expect("resolution event");
        let resolution: UserInputResolved =
            serde_json::from_str(&resolved.content).expect("resolution content");
        assert_eq!(resolution.request_event_id, request_event_id);
        assert!(
            tokio::fs::try_exists(resolution_dir.join("malformed.invalid"))
                .await
                .expect("inspect quarantined resolution")
        );
        restarted.stop_test_workers().await;
        let _ = tokio::fs::remove_dir_all(resolution_dir).await;
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_timeout_covers_requests_still_waiting_for_admission() {
        let owner = Keys::generate();
        let agent = Keys::generate();
        let (publisher, _published) = RelayEventPublisher::test_pair();
        let runtime = QuestionRuntime::new(
            publisher,
            agent.clone(),
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            RestClient {
                http: reqwest::Client::new(),
                base_url: "http://127.0.0.1:0".to_string(),
                keys: agent,
                auth_tag_json: None,
            },
        );
        let request = nostr::EventBuilder::text_note("pending request")
            .sign_with_keys(&runtime.keys)
            .expect("sign pending request");
        let request_event_id = request.id.to_hex();
        let pending_dir = pending_request_outbox_dir(&runtime).expect("pending directory");
        let resolution_dir = resolution_outbox_dir(&runtime).expect("resolution directory");
        let _ = tokio::fs::remove_dir_all(&resolution_dir).await;
        let request_path = persist_outbox_event(&pending_dir, &request)
            .await
            .expect("persist pending request");
        let (sender, _receiver) = oneshot::channel();
        let (admission_tx, _admission_rx) = watch::channel(false);
        runtime.pending.lock().await.insert(
            request_event_id,
            PendingRequest {
                channel_id: Uuid::new_v4(),
                intended_owner_pubkey: owner.public_key().to_hex(),
                sender,
                admission_tx,
                resolution_started: false,
                _lease: None,
            },
        );

        assert!(!runtime.shutdown_pending().await);
        assert!(tokio::fs::try_exists(&request_path)
            .await
            .expect("inspect durable pending request"));
        runtime.stop_test_workers().await;
        let _ = tokio::fs::remove_dir_all(resolution_dir).await;
    }

    #[test]
    fn normalizes_select_and_freeform() {
        let schema = serde_json::json!({"type":"object","properties":{
            "question_0":{"type":"string","title":"Pick","oneOf":[
                {"const":"yes","title":"Yes","description":"Do it"},
                {"const":"no","title":"No","description":"Don't"}
            ]},
            "question_1":{"type":"string","description":"Why?"}
        }});
        let form = normalize_form(&schema).expect("supported");
        assert_eq!(form.questions[0].options[0].value, "yes");
        assert_eq!(form.questions[0].options[0].label, "Yes");
        assert!(form.questions[1].options.is_empty());
    }

    #[test]
    fn reconstructs_custom_and_multi_select() {
        let schema = serde_json::json!({"type":"object","properties":{
            "question_0":{"type":"string","oneOf":[{"const":"a"}]},
            "question_0_custom":{"type":"string"},
            "question_1":{"type":"array","items":{"anyOf":[{"const":"x"}]}}
        }});
        let form = normalize_form(&schema).expect("supported");
        let answers = BTreeMap::from([
            ("q0".into(), Some(UserInputAnswer::Text("custom".into()))),
            ("q1".into(), Some(UserInputAnswer::Multi(vec!["x".into()]))),
        ]);
        let content = reconstruct_content(&form, &answers).expect("answers");
        assert_eq!(content["question_0_custom"], "custom");
        assert_eq!(content["question_1"], serde_json::json!(["x"]));
    }

    #[test]
    fn reconstructs_matching_plain_option_on_native_key() {
        let schema = serde_json::json!({"type":"object","properties":{
            "question_0":{"type":"string","oneOf":[
                {"const":"production","title":"Production"}
            ]},
            "question_0_custom":{"type":"string"}
        }});
        let form = normalize_form(&schema).expect("supported");
        let by_value = BTreeMap::from([(
            "q0".into(),
            Some(UserInputAnswer::Text("production".into())),
        )]);
        let by_label = BTreeMap::from([(
            "q0".into(),
            Some(UserInputAnswer::Text("Production".into())),
        )]);
        assert_eq!(
            reconstruct_content(&form, &by_value).expect("value")["question_0"],
            "production"
        );
        assert_eq!(
            reconstruct_content(&form, &by_label).expect("label")["question_0"],
            "production"
        );
        assert!(!reconstruct_content(&form, &by_value)
            .expect("value")
            .contains_key("question_0_custom"));
    }

    #[test]
    fn rejects_unsupported_schema() {
        assert!(normalize_form(&serde_json::json!({
            "type":"object",
            "properties":{"x":{"type":"number"}}
        }))
        .is_none());
    }

    #[test]
    fn preserves_natural_question_order() {
        let mut properties = serde_json::Map::new();
        for index in 0..12 {
            properties.insert(
                format!("question_{index}"),
                serde_json::json!({"type":"string","title":format!("Q{index}")}),
            );
        }
        let form = normalize_form(&serde_json::json!({"type":"object","properties":properties}))
            .expect("supported");
        let headers = form
            .questions
            .iter()
            .map(|question| question.header.as_str())
            .collect::<Vec<_>>();
        assert_eq!(headers[2], "Q2");
        assert_eq!(headers[10], "Q10");
    }

    #[test]
    fn omits_unanswered_optional_fields_but_requires_required_fields() {
        let schema = serde_json::json!({
            "type":"object",
            "properties":{
                "question_0":{"type":"string","title":"Required"},
                "question_1":{"type":"string","title":"Optional"}
            },
            "required":["question_0"]
        });
        let form = normalize_form(&schema).expect("supported");
        let partial = BTreeMap::from([("q0".into(), Some(UserInputAnswer::Text("answer".into())))]);
        assert!(reconstruct_content(&form, &partial).is_some());
        let missing_required =
            BTreeMap::from([("q1".into(), Some(UserInputAnswer::Text("answer".into())))]);
        assert!(reconstruct_content(&form, &missing_required).is_none());
    }

    #[test]
    fn required_round_trips_and_old_question_events_default_to_false() {
        let schema = serde_json::json!({
            "type":"object",
            "properties":{"question_0":{"type":"string"}},
            "required":["question_0"]
        });
        let form = normalize_form(&schema).expect("supported");
        assert!(form.questions[0].required);
        let encoded = serde_json::to_string(&form.questions[0]).expect("question JSON");
        assert!(
            serde_json::from_str::<UserInputQuestion>(&encoded)
                .expect("question round trip")
                .required
        );
        let old = r#"{"id":"q0","header":"Pick","question":"Choose","options":[]}"#;
        assert!(
            !serde_json::from_str::<UserInputQuestion>(old)
                .expect("old question JSON")
                .required
        );
    }

    #[tokio::test]
    async fn ignores_non_owner_then_accepts_first_owner_answer() {
        let channel_id = Uuid::new_v4();
        let owner = Keys::generate();
        let agent = Keys::generate();
        let stranger = Keys::generate();
        let (publisher, mut published) = RelayEventPublisher::test_pair();
        let runtime = QuestionRuntime::new(
            publisher,
            agent.clone(),
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            RestClient {
                http: reqwest::Client::new(),
                base_url: "http://127.0.0.1:0".to_string(),
                keys: agent,
                auth_tag_json: None,
            },
        );
        let form = normalize_form(&serde_json::json!({
            "type":"object",
            "properties":{"question_0":{"type":"string","oneOf":[{"const":"yes"}]}}
        }))
        .expect("supported");
        let (event_id, mut receiver) = runtime
            .publish(
                channel_id,
                &test_thread_ref(),
                "session",
                "turn",
                Engine::Claude,
                form,
                "request",
                Some("Choose"),
                Some("tool"),
            )
            .await
            .expect("publish");
        let request = published.recv().await.expect("request event");
        assert_eq!(event_id, request.id.to_hex());
        let request_content: UserInputRequest =
            serde_json::from_str(&request.content).expect("request contract");
        assert_eq!(request_content.message.as_deref(), Some("Choose"));
        assert_eq!(request_content.tool_call_id.as_deref(), Some("tool"));
        let requesting_agent = request.pubkey.to_hex();

        let stranger_answer = buzz_sdk::build_agent_user_input_answer(
            channel_id,
            &event_id,
            &requesting_agent,
            r#"{"q0":"stranger"}"#,
        )
        .expect("answer builder")
        .sign_with_keys(&stranger)
        .expect("signed stranger answer");
        runtime
            .handle_event(&BuzzEvent {
                channel_id,
                event: stranger_answer,
            })
            .await;
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut receiver)
                .await
                .is_err()
        );

        let wrong_relation_answer = buzz_sdk::build_agent_user_input_answer(
            channel_id,
            &event_id,
            &stranger.public_key().to_hex(),
            r#"{"q0":"wrong-relation"}"#,
        )
        .expect("answer builder")
        .sign_with_keys(&owner)
        .expect("signed owner answer");
        runtime
            .handle_event(&BuzzEvent {
                channel_id,
                event: wrong_relation_answer,
            })
            .await;
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut receiver)
                .await
                .is_err()
        );

        let owner_answer = buzz_sdk::build_agent_user_input_answer(
            channel_id,
            &event_id,
            &requesting_agent,
            r#"{"q0":"owner"}"#,
        )
        .expect("answer builder")
        .sign_with_keys(&owner)
        .expect("signed owner answer");
        runtime
            .handle_event(&BuzzEvent {
                channel_id,
                event: owner_answer,
            })
            .await;
        assert!(receiver
            .await
            .expect("owner answer received")
            .expect("resolution accepted")
            .is_some());

        let late_answer = buzz_sdk::build_agent_user_input_answer(
            channel_id,
            &event_id,
            &requesting_agent,
            r#"{"q0":"late"}"#,
        )
        .expect("answer builder")
        .sign_with_keys(&owner)
        .expect("signed late answer");
        runtime
            .handle_event(&BuzzEvent {
                channel_id,
                event: late_answer,
            })
            .await;
    }

    #[tokio::test]
    async fn publishes_one_resolution_for_each_terminal_outcome() {
        async fn publish_request(
            channel_id: Uuid,
            owner: &Keys,
        ) -> (
            Arc<QuestionRuntime>,
            mpsc::Receiver<nostr::Event>,
            String,
            oneshot::Receiver<Result<Option<UserInputAnswers>, String>>,
        ) {
            let (publisher, published) = RelayEventPublisher::test_pair();
            let agent = Keys::generate();
            let runtime = QuestionRuntime::new(
                publisher,
                agent.clone(),
                Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
                RestClient {
                    http: reqwest::Client::new(),
                    base_url: "http://127.0.0.1:0".to_string(),
                    keys: agent,
                    auth_tag_json: None,
                },
            );
            let form = normalize_form(&serde_json::json!({
                "type":"object",
                "properties":{"question_0":{"type":"string"}}
            }))
            .expect("supported");
            let (event_id, receiver) = runtime
                .publish(
                    channel_id,
                    &test_thread_ref(),
                    "session",
                    "turn",
                    Engine::Claude,
                    form,
                    "request",
                    None,
                    None,
                )
                .await
                .expect("publish");
            (runtime, published, event_id, receiver)
        }

        async fn resolution(published: &mut mpsc::Receiver<nostr::Event>) -> UserInputResolved {
            let _request = published.recv().await.expect("request event");
            let event = published.recv().await.expect("resolution event");
            assert_eq!(
                event.kind.as_u16() as u32,
                buzz_core::kind::KIND_AGENT_USER_INPUT_RESOLVED
            );
            let p_tag_count = event
                .tags
                .iter()
                .filter(|tag| tag.as_slice().first().is_some_and(|value| value == "p"))
                .count();
            assert_eq!(p_tag_count, 1, "resolution tags: {:?}", event.tags);
            serde_json::from_str(&event.content).expect("resolution contract")
        }

        let channel_id = Uuid::new_v4();
        let owner = Keys::generate();

        let (runtime, mut published, event_id, receiver) =
            publish_request(channel_id, &owner).await;
        let answer = buzz_sdk::build_agent_user_input_answer(
            channel_id,
            &event_id,
            &runtime.keys.public_key().to_hex(),
            r#"{"q0":"answer"}"#,
        )
        .expect("answer builder")
        .sign_with_keys(&owner)
        .expect("answer signature");
        runtime
            .handle_event(&BuzzEvent {
                channel_id,
                event: answer,
            })
            .await;
        assert!(receiver
            .await
            .expect("answer received")
            .expect("resolution accepted")
            .is_some());
        assert_eq!(
            resolution(&mut published).await.outcome,
            UserInputResolutionOutcome::Answered
        );

        let (runtime, mut published, event_id, receiver) =
            publish_request(channel_id, &owner).await;
        let decline = buzz_sdk::build_agent_user_input_answer(
            channel_id,
            &event_id,
            &runtime.keys.public_key().to_hex(),
            r#"{"q0":null}"#,
        )
        .expect("answer builder")
        .sign_with_keys(&owner)
        .expect("answer signature");
        runtime
            .handle_event(&BuzzEvent {
                channel_id,
                event: decline,
            })
            .await;
        assert!(receiver
            .await
            .expect("decline received")
            .expect("resolution accepted")
            .is_some());
        assert_eq!(
            resolution(&mut published).await.outcome,
            UserInputResolutionOutcome::Declined
        );

        let (runtime, mut published, event_id, receiver) =
            publish_request(channel_id, &owner).await;
        runtime.cancel(&event_id).await;
        assert!(receiver
            .await
            .expect("cancel received")
            .expect("resolution accepted")
            .is_none());
        assert_eq!(
            resolution(&mut published).await.outcome,
            UserInputResolutionOutcome::Cancelled
        );

        let (runtime, mut published, _event_id, receiver) =
            publish_request(channel_id, &owner).await;
        runtime.shutdown_pending().await;
        assert!(receiver
            .await
            .expect("shutdown received")
            .expect("resolution accepted")
            .is_none());
        assert_eq!(
            resolution(&mut published).await.outcome,
            UserInputResolutionOutcome::Cancelled
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), published.recv())
                .await
                .is_err()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dead_letter_filesystem_failures_are_bounded_and_retain_the_source() {
        use std::os::unix::fs::PermissionsExt;

        let directory =
            std::env::temp_dir().join(format!("buzz-acp-bounded-dead-letter-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("create dead-letter test directory");
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755))
            .expect("make test spool deliberately unsafe");
        let source = directory.join("request.json");
        std::fs::write(&source, b"persisted").expect("write durable source");

        // The helper's own attempt count and retry delays define boundedness.
        // Leave CI scheduling headroom for descriptor validation and fsync work.
        let rejected = tokio::time::timeout(
            Duration::from_secs(1),
            dead_letter_outbox_until_committed(&source, "bounded test"),
        )
        .await
        .expect("dead-letter retries must be bounded");

        assert_eq!(rejected, None);
        assert!(
            source.exists(),
            "failed move must retain the durable source"
        );
        std::fs::remove_dir_all(directory).expect("remove dead-letter test directory");
    }
}
