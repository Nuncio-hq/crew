use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

use crate::managed_agents::ManagedAgentRuntimeReceipt;
use tauri::AppHandle;

const MAX_RUNTIME_RECEIPT_BYTES: u64 = 16 * 1024;
const MAX_RUNTIME_RECEIPT_ENTRIES: usize = 4096;

use super::{
    append_log_marker, current_instance_id, now_iso, process_belongs_to_us,
    process_has_buzz_marker, process_is_running, terminate_process, terminate_runtime_receipt_with,
    valid_agent_runtime_receipt, ManagedAgentPairRuntime, ManagedAgentRecord,
    ManagedAgentRuntimeKey,
};

pub(crate) fn managed_agent_runtime_keys<T>(
    runtimes: &HashMap<ManagedAgentRuntimeKey, T>,
    pubkey: &str,
) -> Vec<ManagedAgentRuntimeKey> {
    runtimes
        .keys()
        .filter(|key| key.pubkey.eq_ignore_ascii_case(pubkey))
        .cloned()
        .collect()
}

#[cfg(test)]
pub(crate) fn managed_agent_runtime_relay_urls<T>(
    runtimes: &HashMap<ManagedAgentRuntimeKey, T>,
    pubkey: &str,
) -> Vec<String> {
    managed_agent_runtime_keys(runtimes, pubkey)
        .into_iter()
        .map(|key| key.relay_url)
        .collect()
}

/// Stop the single tracked runtime pair at `key`, if present.
///
/// Terminates the child, records the exit code, removes the pair receipt,
/// and appends a stop marker to the pair log. On teardown failure the
/// runtime is reinserted so the pair stays visible and stoppable instead of
/// becoming an invisible orphan. Touches no other pair for the agent and
/// does no record-level stop bookkeeping — callers own that.
fn stop_managed_agent_pair<R: tauri::Runtime, T: FnMut(u32) -> Result<(), String>>(
    app: &AppHandle<R>,
    record: &mut ManagedAgentRecord,
    runtimes: &mut HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
    key: &ManagedAgentRuntimeKey,
    terminate: &mut T,
) -> Result<(), String> {
    let Some(mut runtime) = runtimes.remove(key) else {
        super::super::transport_status::clear_key(app, key);
        return Ok(());
    };
    let result = (|| -> Result<(), String> {
        #[cfg(unix)]
        terminate(runtime.child.id())?;
        #[cfg(windows)]
        match runtime.job.take() {
            Some(job) => drop(job),
            None => runtime
                .child
                .kill()
                .map_err(|error| format!("failed to kill agent process: {error}"))?,
        }
        #[cfg(not(any(unix, windows)))]
        runtime
            .child
            .kill()
            .map_err(|error| format!("failed to kill agent process: {error}"))?;
        #[cfg(windows)]
        let _ = terminate;
        let status = runtime
            .child
            .wait()
            .map_err(|error| format!("failed to wait for agent shutdown: {error}"))?;
        record.last_exit_code = status.code();
        super::super::remove_agent_runtime_receipt(app, key);
        if let Err(error) = append_log_marker(
            &runtime.log_path,
            &format!(
                "=== stopped {} ({}) at {} ===",
                record.name,
                record.pubkey,
                now_iso()
            ),
        ) {
            eprintln!(
                "buzz-desktop: failed to append stop marker for {} on {}: {error}",
                record.pubkey, key.relay_url
            );
        }
        Ok(())
    })();
    if let Err(error) = result {
        // Keep failed teardown visible/manageable instead of orphaning it.
        runtimes.insert(key.clone(), runtime);
        return Err(error);
    }
    super::super::transport_status::clear_key(app, key);
    Ok(())
}

/// Terminate a legacy scalar-PID child (pre-pair records) and remove the
/// agent-scoped pid file. Pair receipts are restored separately.
fn stop_legacy_scalar_pid<R: tauri::Runtime>(
    app: &AppHandle<R>,
    record: &mut ManagedAgentRecord,
) -> Result<(), String> {
    if let Some(pid) = record.runtime_pid {
        if process_is_running(pid)
            && process_belongs_to_us(pid)
            && process_has_buzz_marker(pid, &current_instance_id(app))
        {
            terminate_process(pid)?;
        }
        record.runtime_pid = None;
        record.updated_at = now_iso();
    }
    super::super::remove_agent_pid_file(app, &record.pubkey);
    Ok(())
}

/// Stop the runtime pair this record resolves to for the active workspace
/// (explicit relay pin, else the active workspace relay) — the pair-scoped
/// counterpart of [`stop_managed_agent_process`], which drains every pair.
///
/// Community-scoped surfaces (profile panel, Agents tab, auto-restart) stop
/// through here so stopping an agent in one community never tears down its
/// pairs in other communities. Clears the matching agent session cache
/// (pair-scoped when a pair key resolves). When no pair is tracked for this
/// workspace, legacy scalar-PID cleanup is all that remains; agent-wide
/// deletion uses [`stop_managed_agent_process`] to drain durable receipts.
pub fn stop_managed_agent_workspace_pair(
    app: &AppHandle,
    record: &mut ManagedAgentRecord,
    runtimes: &mut HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
) -> Result<(), String> {
    use tauri::Manager;
    let state = app.state::<crate::app_state::AppState>();
    match super::workspace_pair_key(app, record) {
        Some(pair_key) if runtimes.contains_key(&pair_key) => {
            stop_managed_agent_pair(app, record, runtimes, &pair_key, &mut terminate_process)?;
            state.clear_agent_session_cache(&pair_key);
            super::super::remove_agent_pid_file(app, &record.pubkey);
            let now = now_iso();
            record.runtime_pid = None;
            record.updated_at = now.clone();
            record.last_stopped_at = Some(now);
            record.last_error = None;
            record.last_error_code = None;
        }
        Some(pair_key) => {
            // No tracked pair here — a pubkey-wide cache clear would disturb
            // live pairs in other communities, so stay pair-scoped.
            stop_legacy_scalar_pid(app, record)?;
            super::super::transport_status::clear_key(app, &pair_key);
            state.clear_agent_session_cache(&pair_key);
        }
        None => {
            stop_legacy_scalar_pid(app, record)?;
            super::super::transport_status::clear_pubkey(app, &record.pubkey);
            state.clear_agent_session_caches(&record.pubkey);
        }
    }
    Ok(())
}

fn stop_managed_agent_process_with<R: tauri::Runtime, T: FnMut(u32) -> Result<(), String>>(
    app: &AppHandle<R>,
    record: &mut ManagedAgentRecord,
    runtimes: &mut HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
    mut terminate: T,
) -> Result<(), String> {
    let keys = managed_agent_runtime_keys(runtimes, &record.pubkey);
    if keys.is_empty() {
        stop_untracked_agent_receipts(app, &record.pubkey, &mut terminate)?;
        stop_legacy_scalar_pid(app, record)?;
        super::super::transport_status::clear_pubkey(app, &record.pubkey);
        return Ok(());
    }

    let prior_runtime_pid = record.runtime_pid;
    let prior_updated_at = record.updated_at.clone();
    let prior_last_stopped_at = record.last_stopped_at.clone();
    let prior_last_exit_code = record.last_exit_code;
    let prior_last_error = record.last_error.clone();
    let prior_last_error_code = record.last_error_code.clone();
    let mut errors = Vec::new();
    for key in keys {
        if let Err(error) = stop_managed_agent_pair(app, record, runtimes, &key, &mut terminate) {
            errors.push(format!("{}: {error}", key.relay_url));
        }
    }

    if errors.is_empty() {
        if let Err(error) = stop_untracked_agent_receipts(app, &record.pubkey, &mut terminate) {
            errors.push(error);
        }
    }

    if errors.is_empty() {
        let now = now_iso();
        record.runtime_pid = None;
        record.updated_at = now.clone();
        record.last_stopped_at = Some(now);
        record.last_error = None;
        record.last_error_code = None;
        super::super::remove_agent_pid_file(app, &record.pubkey);
        super::super::transport_status::clear_pubkey(app, &record.pubkey);
        Ok(())
    } else {
        // A failed pair remains in `runtimes`; keep the record-level recovery
        // fields intact as well.  The caller must persist this record and
        // leave its durable deletion intent unresolved, so a retry can still
        // identify the exact instance rather than creating an orphan.
        record.runtime_pid = prior_runtime_pid;
        record.updated_at = prior_updated_at;
        record.last_stopped_at = prior_last_stopped_at;
        record.last_exit_code = prior_last_exit_code;
        record.last_error = prior_last_error.or_else(|| {
            Some(format!(
                "failed to stop one or more managed-agent runtimes: {}",
                errors.join("; ")
            ))
        });
        record.last_error_code = prior_last_error_code;
        Err(format!(
            "failed to stop one or more managed-agent runtimes: {}",
            errors.join("; ")
        ))
    }
}

pub fn stop_managed_agent_process<R: tauri::Runtime>(
    app: &AppHandle<R>,
    record: &mut ManagedAgentRecord,
    runtimes: &mut HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
) -> Result<(), String> {
    stop_managed_agent_process_with(app, record, runtimes, terminate_process)
}

/// Read pair receipts for one record, fail closed on any live receipt that
/// cannot be proven to belong to this desktop instance, then stop the proven
/// pairs. Startup deletion recovery runs before receipt hydration, so this is
/// the receipt-aware stop seam used when the in-memory runtime map is empty.
fn stop_untracked_agent_receipts<R: tauri::Runtime, T: FnMut(u32) -> Result<(), String>>(
    app: &AppHandle<R>,
    pubkey: &str,
    terminate: &mut T,
) -> Result<(), String> {
    let instance_id = current_instance_id(app);
    let receipts = read_agent_runtime_receipts_for_pubkey(app, pubkey)?;
    let mut owned = Vec::new();
    let mut stale = Vec::new();
    for (path, receipt) in receipts {
        if valid_agent_runtime_receipt(&path, &receipt, &instance_id) {
            owned.push((path, receipt));
        } else if process_is_running(receipt.pid) {
            // A live process with an invalid or foreign receipt is not safe to
            // signal. Keep the local record and durable delete operation so a
            // later retry can recover after the owner proof is repaired.
            return Err(
                "managed-agent runtime receipt could not be validated; deletion remains pending"
                    .into(),
            );
        } else {
            stale.push(path);
        }
    }

    // Preflight every receipt before stopping any pair. A later invalid live
    // receipt must not turn a multi-pair delete into a partially untracked
    // operation merely because an earlier pair was valid.
    for (path, receipt) in owned {
        terminate_runtime_receipt_with(
            &path,
            &receipt,
            &mut *terminate,
            process_is_running,
            super::super::remove_agent_runtime_receipt_path,
        )?;
    }
    for path in stale {
        super::super::remove_agent_runtime_receipt_path(&path);
    }
    Ok(())
}

/// Strictly read receipts attributable to `pubkey`. The regular startup
/// sweep intentionally ignores malformed JSON, but deletion must preserve a
/// retry witness when a target-named receipt is unreadable.
fn read_agent_runtime_receipts_for_pubkey<R: tauri::Runtime>(
    app: &AppHandle<R>,
    pubkey: &str,
) -> Result<Vec<(PathBuf, ManagedAgentRuntimeReceipt)>, String> {
    let dir = super::super::managed_agents_base_dir(app)?.join("agent-pids");
    match fs::symlink_metadata(&dir) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(
                "managed-agent runtime receipt directory is unsafe; deletion remains pending"
                    .into(),
            );
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("managed-agent runtime receipts are unavailable".into()),
    }
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("managed-agent runtime receipts are unavailable".into()),
    };
    let mut paths = Vec::new();
    for (index, entry) in entries.enumerate() {
        // Count every directory entry, including unrelated extensions, before
        // filtering. An incomplete scan must not authorize any termination.
        if index >= MAX_RUNTIME_RECEIPT_ENTRIES {
            return Err(
                "managed-agent runtime receipt scan exceeded its entry limit; deletion remains pending"
                    .into(),
            );
        }
        let entry = entry.map_err(|_| "managed-agent runtime receipts are unavailable")?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let claims_pubkey = receipt_path_claims_pubkey(&path, pubkey);
        paths.push((path, claims_pubkey));
    }
    // Check target-named receipts first so a corrupt target cannot be hidden
    // behind unrelated files in a large receipt directory.
    paths.sort_by_key(|(_, claims_pubkey)| !*claims_pubkey);

    let mut receipts = Vec::new();
    for (path, path_claims_pubkey) in paths {
        // `path` came from this exact directory's `read_dir`; retain the
        // lexical containment check because this reader is used by deletion,
        // where a path outside `agent-pids/` must never become a termination
        // target even if a filesystem race changes an entry after discovery.
        if path.parent() != Some(dir.as_path()) {
            if path_claims_pubkey {
                return Err(
                    "managed-agent runtime receipt escaped agent-pids; deletion remains pending"
                        .into(),
                );
            }
            continue;
        }

        // Never follow a receipt symlink or accept a socket/FIFO/directory as
        // a receipt. The target-named case is a durable deletion failure; an
        // unrelated malformed entry can be ignored without widening the
        // target's recovery surface.
        let link_metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) if path_claims_pubkey => {
                return Err(
                    "managed-agent runtime receipt is unreadable; deletion remains pending".into(),
                );
            }
            Err(_) => continue,
        };
        if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
            if path_claims_pubkey {
                return Err(
                    "managed-agent runtime receipt is not a regular file; deletion remains pending"
                        .into(),
                );
            }
            continue;
        }

        // `O_NOFOLLOW` closes the check-then-open symlink race. The descriptor
        // metadata below is the authoritative object check, so a replacement
        // with a directory or other non-regular node is also rejected.
        let mut file = match open_runtime_receipt(&path) {
            Ok(file) => file,
            Err(_) if path_claims_pubkey => {
                return Err(
                    "managed-agent runtime receipt is unreadable; deletion remains pending".into(),
                );
            }
            Err(_) => continue,
        };
        let metadata = match file.metadata() {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) if path_claims_pubkey => {
                return Err(
                    "managed-agent runtime receipt is not a regular file; deletion remains pending"
                        .into(),
                );
            }
            Ok(_) => continue,
            Err(_) if path_claims_pubkey => {
                return Err(
                    "managed-agent runtime receipt is unreadable; deletion remains pending".into(),
                );
            }
            Err(_) => continue,
        };
        if metadata.len() > MAX_RUNTIME_RECEIPT_BYTES {
            if path_claims_pubkey {
                return Err(
                    "managed-agent runtime receipt is too large; deletion remains pending".into(),
                );
            }
            continue;
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        if file
            .by_ref()
            .take(MAX_RUNTIME_RECEIPT_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
        {
            if path_claims_pubkey {
                return Err(
                    "managed-agent runtime receipt is unreadable; deletion remains pending".into(),
                );
            }
            continue;
        }
        if bytes.len() as u64 > MAX_RUNTIME_RECEIPT_BYTES {
            if path_claims_pubkey {
                return Err(
                    "managed-agent runtime receipt is too large; deletion remains pending".into(),
                );
            }
            continue;
        }
        let receipt = match serde_json::from_slice::<ManagedAgentRuntimeReceipt>(&bytes) {
            Ok(receipt) => receipt,
            Err(_) if path_claims_pubkey => {
                return Err(
                    "managed-agent runtime receipt is corrupt; deletion remains pending".into(),
                );
            }
            Err(_) => continue,
        };
        if path_claims_pubkey || receipt.key.pubkey.eq_ignore_ascii_case(pubkey) {
            receipts.push((path, receipt));
        }
    }
    Ok(receipts)
}

fn open_runtime_receipt(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}

fn receipt_path_claims_pubkey(path: &Path, pubkey: &str) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".json"))
        .and_then(|stem| stem.split_once("__").map(|(candidate, _)| candidate))
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(pubkey))
}

#[cfg(all(test, unix))]
#[path = "stop_failure_tests.rs"]
mod stop_failure_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_preserving_restart_targets_exact_original_relays() {
        let agent = "aa".repeat(32);
        let other = "bb".repeat(32);
        let first = ManagedAgentRuntimeKey::new(&agent, "wss://one.example").unwrap();
        let second = ManagedAgentRuntimeKey::new(&agent, "wss://two.example").unwrap();
        let unrelated = ManagedAgentRuntimeKey::new(other, "wss://fallback.example").unwrap();
        let runtimes = HashMap::from([(first, ()), (second, ()), (unrelated, ())]);

        let mut relays = managed_agent_runtime_relay_urls(&runtimes, &agent);
        relays.sort();
        assert_eq!(
            relays,
            vec![
                "wss://one.example".to_string(),
                "wss://two.example".to_string()
            ]
        );
    }

    #[test]
    fn pair_scoped_selection_targets_only_the_exact_pair() {
        // stop_managed_agent_workspace_pair resolves one key and removes only
        // that map entry: the same agent's pair on another relay and other
        // agents' pairs must survive a pair-scoped stop.
        let agent = "aa".repeat(32);
        let other = "bb".repeat(32);
        let viewed = ManagedAgentRuntimeKey::new(&agent, "wss://one.example").unwrap();
        let elsewhere = ManagedAgentRuntimeKey::new(&agent, "wss://two.example").unwrap();
        let unrelated = ManagedAgentRuntimeKey::new(other, "wss://one.example").unwrap();
        let mut runtimes = HashMap::from([
            (viewed.clone(), ()),
            (elsewhere.clone(), ()),
            (unrelated.clone(), ()),
        ]);

        // Non-canonical spelling of the viewed workspace relay resolves to
        // the same canonical key that spawn stamped.
        let resolved = ManagedAgentRuntimeKey::new(&agent, "WSS://One.Example:443/").unwrap();
        assert_eq!(resolved, viewed);
        assert!(runtimes.remove(&resolved).is_some());
        assert!(runtimes.contains_key(&elsewhere));
        assert!(runtimes.contains_key(&unrelated));
    }

    #[test]
    fn agent_wide_selection_drains_every_pair_only_for_that_agent() {
        let agent = "aa".repeat(32);
        let other = "bb".repeat(32);
        let first = ManagedAgentRuntimeKey::new(&agent, "wss://one.example").unwrap();
        let second = ManagedAgentRuntimeKey::new(&agent, "wss://two.example").unwrap();
        let unrelated = ManagedAgentRuntimeKey::new(other, "wss://one.example").unwrap();
        let runtimes = HashMap::from([(first.clone(), ()), (second.clone(), ()), (unrelated, ())]);

        let mut selected = managed_agent_runtime_keys(&runtimes, &agent);
        selected.sort_by(|left, right| left.relay_url.cmp(&right.relay_url));
        assert_eq!(selected, vec![first, second]);
    }
}
