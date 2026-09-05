//! Honest per-engine compaction detection (issue #173 / D-050).
//!
//! Detection is an interface with per-engine adapters. The owner-facing
//! `compaction_count` increments **only** on a real signal. Engines with no
//! proven adapter stay `Unknown` forever — a fabricated count is worse than
//! none. Adapter parse failure fails loud into `Unavailable`.
//!
//! `used` / `contextLimit` deltas are never a detection source (decorative
//! UI only). A universal turn-count safety net (default 100) triggers the
//! same aging awareness without inventing a compaction number.

use serde::{Deserialize, Serialize};

/// Where a compaction observation came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompactionSignalSource {
    /// ACP `session/update` (Hermes provenance, Codex `context_compacted`).
    AcpNotification,
    /// Engine hook (`_PostCompact`, Claude `PreCompact` glue).
    Hook,
    /// On-disk transcript marker (Codex rollout JSONL).
    TranscriptMarker,
    /// No proven signal for this engine/version.
    None,
}

/// Whether the harness may show a numeric compaction count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CompactionSignalAvailability {
    /// At least one real signal was observed for this session generation.
    Known,
    /// No proven adapter / no signal yet — never show a number.
    #[default]
    Unknown,
    /// Adapter existed but parse/format failed — freeze, never miscount.
    Unavailable,
}

impl CompactionSignalAvailability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Known => "known",
            Self::Unknown => "unknown",
            Self::Unavailable => "unavailable",
        }
    }

    #[allow(dead_code)]
    pub fn from_wire(value: &str) -> Self {
        match value {
            "known" => Self::Known,
            "unavailable" => Self::Unavailable,
            _ => Self::Unknown,
        }
    }

    /// Owner UI may render a compaction number only when `Known`.
    pub fn may_show_count(self) -> bool {
        matches!(self, Self::Known)
    }
}

/// Owner-facing aging projection (never Lost contact / Possibly stalled).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAgingState {
    pub aging: bool,
    pub reason: Option<&'static str>,
    pub compaction_count: u32,
    pub signal: CompactionSignalAvailability,
    pub session_turn_count: u32,
}

/// Default compaction threshold (configurable 1–10).
pub const DEFAULT_COMPACTION_THRESHOLD: u32 = 3;
/// Default turn-count safety net.
pub const DEFAULT_TURN_AGING_THRESHOLD: u32 = 100;

/// Clamp owner-configurable compaction threshold into the accepted range.
pub fn clamp_compaction_threshold(value: u32) -> u32 {
    value.clamp(1, 10)
}

/// Apply a real compaction observation onto the running counters.
///
/// Rules:
/// - `Unavailable` is sticky — further signals do not resume counting.
/// - `source = None` never mutates counts (stays Unknown).
/// - Absolute depth (Hermes / Codex) takes `max(current, observed_depth)`.
/// - Hook increments by one when `observed_depth` is `None`.
pub fn apply_compaction_signal(
    current_count: u32,
    current_signal: CompactionSignalAvailability,
    source: CompactionSignalSource,
    observed_depth: Option<u32>,
) -> (u32, CompactionSignalAvailability) {
    if matches!(current_signal, CompactionSignalAvailability::Unavailable) {
        return (current_count, CompactionSignalAvailability::Unavailable);
    }
    match source {
        CompactionSignalSource::None => (current_count, current_signal),
        CompactionSignalSource::AcpNotification
        | CompactionSignalSource::TranscriptMarker
        | CompactionSignalSource::Hook => {
            let next = match observed_depth {
                Some(depth) => current_count.max(depth),
                None => current_count.saturating_add(1),
            };
            (next, CompactionSignalAvailability::Known)
        }
    }
}

/// Mark the counter unavailable after adapter parse failure (fail loud).
#[allow(dead_code)] // called from ledger + Codex transcript fail-loud path
pub fn mark_compaction_unavailable(current_count: u32) -> (u32, CompactionSignalAvailability) {
    (current_count, CompactionSignalAvailability::Unavailable)
}

/// Project benign aging from honest counters + thresholds.
///
/// - Known + count ≥ threshold → aging with compaction reason (number shown).
/// - Unknown/Unavailable + turns ≥ turn threshold → aging with turn reason
///   (no compaction number).
/// - Never fabricates a compaction count for Unknown/Unavailable.
pub fn project_session_aging(
    compaction_count: u32,
    signal: CompactionSignalAvailability,
    session_turn_count: u32,
    compaction_threshold: u32,
    turn_threshold: u32,
) -> SessionAgingState {
    let threshold = clamp_compaction_threshold(compaction_threshold);
    let turn_threshold = turn_threshold.max(1);

    if signal.may_show_count() && compaction_count >= threshold {
        return SessionAgingState {
            aging: true,
            reason: Some("compaction_threshold"),
            compaction_count,
            signal,
            session_turn_count,
        };
    }

    if !signal.may_show_count() && session_turn_count >= turn_threshold {
        return SessionAgingState {
            aging: true,
            reason: Some("turn_count_net"),
            compaction_count: 0,
            signal,
            session_turn_count,
        };
    }

    SessionAgingState {
        aging: false,
        reason: None,
        compaction_count: if signal.may_show_count() {
            compaction_count
        } else {
            0
        },
        signal,
        session_turn_count,
    }
}

/// Banner copy helpers (desktop + harness share the contract).
#[allow(dead_code)] // contract-tested; desktop mirrors the same copy rules
pub fn aging_banner_reason_copy(state: &SessionAgingState) -> Option<String> {
    if !state.aging {
        return None;
    }
    match state.reason {
        Some("compaction_threshold") => Some(format!(
            "session compacted {}× — memory may be degraded",
            state.compaction_count
        )),
        Some("turn_count_net") => Some(format!(
            "long session ({}+ turns)",
            state.session_turn_count
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_only_moves_on_real_signal() {
        let (count, signal) = apply_compaction_signal(
            0,
            CompactionSignalAvailability::Unknown,
            CompactionSignalSource::None,
            Some(3),
        );
        assert_eq!(count, 0);
        assert_eq!(signal, CompactionSignalAvailability::Unknown);

        let (count, signal) = apply_compaction_signal(
            0,
            CompactionSignalAvailability::Unknown,
            CompactionSignalSource::AcpNotification,
            Some(2),
        );
        assert_eq!(count, 2);
        assert_eq!(signal, CompactionSignalAvailability::Known);
    }

    #[test]
    fn unknown_stays_unknown_without_signal() {
        let aging = project_session_aging(
            0,
            CompactionSignalAvailability::Unknown,
            50,
            DEFAULT_COMPACTION_THRESHOLD,
            DEFAULT_TURN_AGING_THRESHOLD,
        );
        assert!(!aging.aging);
        assert_eq!(aging.compaction_count, 0);
        assert!(!aging.signal.may_show_count());
    }

    #[test]
    fn adapter_parse_failure_freezes_into_unavailable() {
        let (count, _signal) = apply_compaction_signal(
            1,
            CompactionSignalAvailability::Known,
            CompactionSignalSource::TranscriptMarker,
            Some(2),
        );
        assert_eq!(count, 2);
        let (count, signal) = mark_compaction_unavailable(count);
        assert_eq!(signal, CompactionSignalAvailability::Unavailable);
        let (count, signal) = apply_compaction_signal(
            count,
            signal,
            CompactionSignalSource::TranscriptMarker,
            Some(9),
        );
        assert_eq!(count, 2, "unavailable must not resume counting");
        assert_eq!(signal, CompactionSignalAvailability::Unavailable);
    }

    #[test]
    fn threshold_crossing_projects_aging_once_semantics() {
        let below = project_session_aging(2, CompactionSignalAvailability::Known, 10, 3, 100);
        assert!(!below.aging);

        let at = project_session_aging(3, CompactionSignalAvailability::Known, 10, 3, 100);
        assert!(at.aging);
        assert_eq!(at.reason, Some("compaction_threshold"));
        assert_eq!(at.compaction_count, 3);
    }

    #[test]
    fn turn_count_net_for_signal_less_engine() {
        let aging = project_session_aging(0, CompactionSignalAvailability::Unknown, 100, 3, 100);
        assert!(aging.aging);
        assert_eq!(aging.reason, Some("turn_count_net"));
        assert_eq!(aging.compaction_count, 0);
        let copy = aging_banner_reason_copy(&aging).expect("copy");
        assert!(copy.contains("100+ turns"));
        assert!(!copy.contains("compacted"));
    }

    #[test]
    fn reset_clears_aging_projection() {
        let after_reset =
            project_session_aging(0, CompactionSignalAvailability::Unknown, 0, 3, 100);
        assert!(!after_reset.aging);
        assert_eq!(after_reset.compaction_count, 0);
    }

    #[test]
    fn hook_increments_by_one() {
        let (count, signal) = apply_compaction_signal(
            0,
            CompactionSignalAvailability::Unknown,
            CompactionSignalSource::Hook,
            None,
        );
        assert_eq!(count, 1);
        assert_eq!(signal, CompactionSignalAvailability::Known);
    }

    #[test]
    fn compaction_threshold_clamped_1_to_10() {
        assert_eq!(clamp_compaction_threshold(0), 1);
        assert_eq!(clamp_compaction_threshold(3), 3);
        assert_eq!(clamp_compaction_threshold(99), 10);
    }
}
