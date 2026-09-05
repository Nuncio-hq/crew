//! App-scoped alive-interval ledger for observed-time idle (#174).
//!
//! Heartbeat persists an "alive until" stamp. Gaps larger than 2× the heartbeat
//! granule close the previous interval at the last stamp and open a new one —
//! nights with the lid closed and days away accumulate nothing.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::worktree_storage_policy::{
    AliveInterval, ALIVE_RETENTION_SECS, HEARTBEAT_GAP_CLOSE_SECS, HEARTBEAT_GRANULE_SECS,
};

const LEDGER_FILE: &str = "worktree-storage-alive.json";
const LEDGER_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AliveLedgerFile {
    version: u32,
    /// Closed intervals only.
    intervals: Vec<AliveInterval>,
    /// Open interval start, when the app is currently considered alive.
    open_start: Option<i64>,
    /// Last heartbeat stamp ("alive until").
    alive_until: Option<i64>,
    /// Most recent closed gap (absence) in seconds — for banner honesty.
    recent_absence_secs: i64,
    /// Configurable observed-idle threshold (default 48h).
    idle_threshold_secs: i64,
}

impl Default for AliveLedgerFile {
    fn default() -> Self {
        Self {
            version: LEDGER_VERSION,
            intervals: Vec::new(),
            open_start: None,
            alive_until: None,
            recent_absence_secs: 0,
            idle_threshold_secs: super::worktree_storage_policy::DEFAULT_IDLE_THRESHOLD_SECS,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeStorageAliveStatus {
    pub intervals: Vec<AliveInterval>,
    pub recent_absence_secs: i64,
    pub idle_threshold_secs: i64,
    pub heartbeat_granule_secs: i64,
    pub now: i64,
}

fn ledger_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("App data directory unavailable: {error}"))?;
    fs::create_dir_all(&dir).map_err(|error| format!("Could not create app data dir: {error}"))?;
    Ok(dir.join(LEDGER_FILE))
}

fn read_ledger(path: &PathBuf) -> AliveLedgerFile {
    let Ok(bytes) = fs::read(path) else {
        return AliveLedgerFile::default();
    };
    serde_json::from_slice::<AliveLedgerFile>(&bytes).unwrap_or_default()
}

fn write_ledger(path: &PathBuf, ledger: &AliveLedgerFile) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(ledger)
        .map_err(|error| format!("Could not serialize alive ledger: {error}"))?;
    fs::write(path, bytes).map_err(|error| format!("Could not persist alive ledger: {error}"))
}

/// Apply one heartbeat at `now`. Pure for contract tests; IO wraps it.
fn apply_heartbeat(ledger: &mut AliveLedgerFile, now: i64) {
    match (ledger.open_start, ledger.alive_until) {
        (Some(start), Some(until)) => {
            let gap = now.saturating_sub(until);
            if gap > HEARTBEAT_GAP_CLOSE_SECS {
                // Close previous interval at last stamp; open a new one at now.
                if until > start {
                    ledger.intervals.push(AliveInterval { start, end: until });
                }
                ledger.recent_absence_secs = gap;
                ledger.open_start = Some(now);
                ledger.alive_until = Some(now);
            } else {
                ledger.alive_until = Some(now.max(until));
            }
        }
        _ => {
            ledger.open_start = Some(now);
            ledger.alive_until = Some(now);
        }
    }
    prune_intervals(ledger, now);
}

fn prune_intervals(ledger: &mut AliveLedgerFile, now: i64) {
    let cutoff = now.saturating_sub(ALIVE_RETENTION_SECS);
    ledger.intervals.retain(|interval| interval.end >= cutoff);
    // Coalesce adjacent/overlapping intervals.
    if ledger.intervals.len() < 2 {
        return;
    }
    ledger.intervals.sort_by_key(|interval| interval.start);
    let mut coalesced = Vec::with_capacity(ledger.intervals.len());
    for interval in ledger.intervals.drain(..) {
        if let Some(last) = coalesced.last_mut() {
            let AliveInterval { start: _, end } = *last;
            if interval.start <= end + HEARTBEAT_GRANULE_SECS {
                last.end = end.max(interval.end);
                continue;
            }
        }
        coalesced.push(interval);
    }
    ledger.intervals = coalesced;
}

/// Materialize closed + open intervals for `observed_idle` queries.
fn materialize_intervals(ledger: &AliveLedgerFile) -> Vec<AliveInterval> {
    let mut out = ledger.intervals.clone();
    if let (Some(start), Some(end)) = (ledger.open_start, ledger.alive_until) {
        if end > start {
            out.push(AliveInterval { start, end });
        } else if end == start {
            // Zero-width open tick still counts the instant as alive.
            out.push(AliveInterval {
                start,
                end: end + 1,
            });
        }
    }
    out
}

#[tauri::command]
pub fn touch_worktree_storage_alive(app: AppHandle) -> Result<WorktreeStorageAliveStatus, String> {
    let path = ledger_path(&app)?;
    let mut ledger = read_ledger(&path);
    let now = chrono_now();
    apply_heartbeat(&mut ledger, now);
    write_ledger(&path, &ledger)?;
    Ok(status_from_ledger(&ledger, now))
}

#[tauri::command]
pub fn get_worktree_storage_alive(app: AppHandle) -> Result<WorktreeStorageAliveStatus, String> {
    let path = ledger_path(&app)?;
    let ledger = read_ledger(&path);
    Ok(status_from_ledger(&ledger, chrono_now()))
}

#[tauri::command]
pub fn set_worktree_storage_idle_threshold(
    app: AppHandle,
    idle_threshold_secs: i64,
) -> Result<WorktreeStorageAliveStatus, String> {
    if idle_threshold_secs < 3600 {
        return Err("Idle threshold must be at least 1 hour.".to_string());
    }
    let path = ledger_path(&app)?;
    let mut ledger = read_ledger(&path);
    ledger.idle_threshold_secs = idle_threshold_secs;
    write_ledger(&path, &ledger)?;
    Ok(status_from_ledger(&ledger, chrono_now()))
}

pub(crate) fn load_intervals_and_threshold(
    app: &AppHandle,
) -> Result<(Vec<AliveInterval>, i64, i64), String> {
    let path = ledger_path(app)?;
    let ledger = read_ledger(&path);
    Ok((
        materialize_intervals(&ledger),
        ledger.idle_threshold_secs,
        ledger.recent_absence_secs,
    ))
}

fn status_from_ledger(ledger: &AliveLedgerFile, now: i64) -> WorktreeStorageAliveStatus {
    WorktreeStorageAliveStatus {
        intervals: materialize_intervals(ledger),
        recent_absence_secs: ledger.recent_absence_secs,
        idle_threshold_secs: ledger.idle_threshold_secs,
        heartbeat_granule_secs: HEARTBEAT_GRANULE_SECS,
        now,
    }
}

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::worktree_storage_policy::observed_idle_secs;

    #[test]
    fn gap_closes_interval_and_records_absence() {
        let mut ledger = AliveLedgerFile::default();
        apply_heartbeat(&mut ledger, 1_000);
        apply_heartbeat(&mut ledger, 1_060);
        apply_heartbeat(&mut ledger, 1_120);
        // 7-day gap.
        apply_heartbeat(&mut ledger, 1_120 + 7 * 86_400);
        assert_eq!(ledger.intervals.len(), 1);
        assert_eq!(ledger.intervals[0].start, 1_000);
        assert_eq!(ledger.intervals[0].end, 1_120);
        assert!(ledger.recent_absence_secs >= 7 * 86_400 - HEARTBEAT_GAP_CLOSE_SECS);
        let intervals = materialize_intervals(&ledger);
        let observed = observed_idle_secs(1_000, &intervals, 1_120 + 7 * 86_400);
        // Only the pre-gap window (~120s) plus the reopen tick.
        assert!(observed < 200);
    }

    #[test]
    fn continuous_heartbeats_extend_open_interval() {
        let mut ledger = AliveLedgerFile::default();
        for t in (0..=600).step_by(60) {
            apply_heartbeat(&mut ledger, t);
        }
        assert!(ledger.intervals.is_empty());
        assert_eq!(ledger.open_start, Some(0));
        assert_eq!(ledger.alive_until, Some(600));
        let intervals = materialize_intervals(&ledger);
        assert_eq!(observed_idle_secs(0, &intervals, 600), 600);
    }

    #[test]
    fn prune_drops_intervals_beyond_retention() {
        let mut ledger = AliveLedgerFile::default();
        let now = ALIVE_RETENTION_SECS + 10_000;
        ledger.intervals.push(AliveInterval { start: 0, end: 100 });
        ledger.intervals.push(AliveInterval {
            start: now - 1_000,
            end: now - 500,
        });
        prune_intervals(&mut ledger, now);
        assert_eq!(ledger.intervals.len(), 1);
        assert_eq!(ledger.intervals[0].start, now - 1_000);
    }
}
