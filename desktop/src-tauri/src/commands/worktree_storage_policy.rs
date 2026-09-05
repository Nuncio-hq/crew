//! Observed-time idle policy for worktree storage reclaim (#174).
//!
//! Pure classification beside the alive-interval ledger. Does not authorize
//! mutation — Lean/Hibernate still execute through #72 cache clear / evict.

use serde::{Deserialize, Serialize};

/// Default idle candidacy threshold: 48 observed hours.
pub const DEFAULT_IDLE_THRESHOLD_SECS: i64 = 48 * 60 * 60;

/// Heartbeat granularity for the app-scoped alive ledger (~60s).
pub const HEARTBEAT_GRANULE_SECS: i64 = 60;

/// Gap larger than this closes the previous alive interval.
pub const HEARTBEAT_GAP_CLOSE_SECS: i64 = HEARTBEAT_GRANULE_SECS * 2;

/// Bound ledger retention comfortably above any configurable threshold.
pub const ALIVE_RETENTION_SECS: i64 = 90 * 24 * 60 * 60;

/// One closed or open app-alive window `[start, end]` in unix seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AliveInterval {
    pub start: i64,
    pub end: i64,
}

/// Registry PR linkage used for candidacy (never git ancestry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrLinkState {
    Merged { number: u64 },
    ClosedUnmerged { number: u64 },
    Open { number: u64 },
    None,
}

/// Action tier mapped to existing #72 primitives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReclaimTier {
    Lean,
    Hibernate,
}

/// Pure policy inputs. Authorization still revalidates at execution time.
#[derive(Debug, Clone)]
pub struct PolicyInput<'a> {
    pub last_used_at: Option<i64>,
    pub intervals: &'a [AliveInterval],
    pub now: i64,
    pub idle_threshold_secs: i64,
    pub pr: PrLinkState,
    pub dirty: bool,
    pub busy: bool,
    pub branch_pushed: bool,
    pub can_clear_cache: bool,
    pub can_evict: bool,
    pub clear_cache_refusal: Option<&'a str>,
    pub eviction_refusal: Option<&'a str>,
    pub cache_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReclaimClassification {
    pub candidate: bool,
    pub tier: Option<ReclaimTier>,
    pub reason: String,
    pub observed_idle_secs: i64,
    pub wall_idle_secs: Option<i64>,
    pub read_only: bool,
    pub refusal_reason: Option<String>,
}

/// Total overlap of alive intervals with `[last_used_at, now]`.
///
/// Gaps while the app/machine were off accrue nothing. Crash loses at most one
/// heartbeat granule (interval ends at the last stamp).
pub fn observed_idle_secs(last_used_at: i64, intervals: &[AliveInterval], now: i64) -> i64 {
    if now <= last_used_at {
        return 0;
    }
    let mut total = 0_i64;
    for interval in intervals {
        if interval.end <= interval.start {
            continue;
        }
        let start = interval.start.max(last_used_at);
        let end = interval.end.min(now);
        if end > start {
            total = total.saturating_add(end - start);
        }
    }
    total
}

/// Classify Lean / Hibernate / non-candidate from observed idle + PR state.
pub fn classify_reclaim_candidate(input: &PolicyInput<'_>) -> ReclaimClassification {
    let observed = match input.last_used_at {
        Some(last) => observed_idle_secs(last, input.intervals, input.now),
        None => 0,
    };
    let wall_idle_secs = input.last_used_at.map(|last| (input.now - last).max(0));

    if input.busy {
        return ReclaimClassification {
            candidate: false,
            tier: None,
            reason: "busy — agent lease held".to_string(),
            observed_idle_secs: observed,
            wall_idle_secs,
            read_only: true,
            refusal_reason: Some(
                input
                    .clear_cache_refusal
                    .unwrap_or("An agent is using this worktree.")
                    .to_string(),
            ),
        };
    }

    if !input.can_clear_cache && !input.can_evict {
        let refusal = input
            .clear_cache_refusal
            .or(input.eviction_refusal)
            .unwrap_or("This worktree cannot be reclaimed.")
            .to_string();
        return ReclaimClassification {
            candidate: false,
            tier: None,
            reason: refusal.clone(),
            observed_idle_secs: observed,
            wall_idle_secs,
            read_only: true,
            refusal_reason: Some(refusal),
        };
    }

    let idle_qualifies = observed > input.idle_threshold_secs;
    let merged = matches!(input.pr, PrLinkState::Merged { .. });
    if !idle_qualifies && !merged {
        let reason = match input.pr {
            PrLinkState::Open { number } => format!("PR #{number} open — not idle enough"),
            PrLinkState::ClosedUnmerged { number } => {
                format!("PR #{number} closed unmerged — not idle enough")
            }
            PrLinkState::None => "not idle enough (observed hours)".to_string(),
            PrLinkState::Merged { .. } => unreachable!(),
        };
        return ReclaimClassification {
            candidate: false,
            tier: None,
            reason,
            observed_idle_secs: observed,
            wall_idle_secs,
            read_only: false,
            refusal_reason: None,
        };
    }

    let qualify_reason = if merged {
        match input.pr {
            PrLinkState::Merged { number } => format!("PR #{number} merged"),
            _ => "PR merged".to_string(),
        }
    } else {
        let hours = observed / 3600;
        format!("idle {hours} observed hrs")
    };

    let hibernate_ok = input.can_evict
        && !input.dirty
        && (merged || input.branch_pushed)
        && input.eviction_refusal.is_none();

    if hibernate_ok {
        let push_note = if merged {
            "clean, merged".to_string()
        } else {
            "clean, pushed".to_string()
        };
        return ReclaimClassification {
            candidate: true,
            tier: Some(ReclaimTier::Hibernate),
            reason: format!("{qualify_reason} — Hibernate: evict ({push_note})"),
            observed_idle_secs: observed,
            wall_idle_secs,
            read_only: false,
            refusal_reason: None,
        };
    }

    if input.can_clear_cache && input.cache_bytes > 0 {
        return ReclaimClassification {
            candidate: true,
            tier: Some(ReclaimTier::Lean),
            reason: format!("{qualify_reason} — Lean: sweep cache"),
            observed_idle_secs: observed,
            wall_idle_secs,
            read_only: false,
            refusal_reason: None,
        };
    }

    if input.can_clear_cache && input.cache_bytes == 0 && !hibernate_ok {
        // Qualifies but nothing reclaimable right now — still not a bulk candidate.
        let detail = if input.dirty {
            "dirty — cache empty; eviction blocked"
        } else if !input.branch_pushed && !merged {
            "cache empty; branch not pushed — eviction blocked"
        } else {
            input
                .eviction_refusal
                .unwrap_or("nothing reclaimable right now")
        };
        return ReclaimClassification {
            candidate: false,
            tier: None,
            reason: detail.to_string(),
            observed_idle_secs: observed,
            wall_idle_secs,
            read_only: input.dirty || input.eviction_refusal.is_some(),
            refusal_reason: if input.dirty {
                Some("dirty — uncommitted changes block eviction".to_string())
            } else {
                input.eviction_refusal.map(str::to_string)
            },
        };
    }

    let refusal = input
        .eviction_refusal
        .or(input.clear_cache_refusal)
        .unwrap_or("This worktree cannot be reclaimed.")
        .to_string();
    ReclaimClassification {
        candidate: false,
        tier: None,
        reason: refusal.clone(),
        observed_idle_secs: observed,
        wall_idle_secs,
        read_only: true,
        refusal_reason: Some(refusal),
    }
}

/// Resolve squash-safe PR linkage from registry rows (never git ancestry).
pub fn pr_link_state(states: &[(u64, &str)]) -> PrLinkState {
    if let Some((number, _)) = states
        .iter()
        .find(|(_, state)| state.eq_ignore_ascii_case("MERGED"))
    {
        return PrLinkState::Merged { number: *number };
    }
    if let Some((number, _)) = states.iter().find(|(_, state)| {
        state.eq_ignore_ascii_case("OPEN") || state.eq_ignore_ascii_case("DRAFT")
    }) {
        return PrLinkState::Open { number: *number };
    }
    if let Some((number, _)) = states
        .iter()
        .find(|(_, state)| state.eq_ignore_ascii_case("CLOSED"))
    {
        return PrLinkState::ClosedUnmerged { number: *number };
    }
    PrLinkState::None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intervals(pairs: &[(i64, i64)]) -> Vec<AliveInterval> {
        pairs
            .iter()
            .map(|(start, end)| AliveInterval {
                start: *start,
                end: *end,
            })
            .collect()
    }

    fn base_input<'a>(
        last_used_at: Option<i64>,
        intervals: &'a [AliveInterval],
        now: i64,
        pr: PrLinkState,
    ) -> PolicyInput<'a> {
        PolicyInput {
            last_used_at,
            intervals,
            now,
            idle_threshold_secs: DEFAULT_IDLE_THRESHOLD_SECS,
            pr,
            dirty: false,
            busy: false,
            branch_pushed: false,
            can_clear_cache: true,
            can_evict: true,
            clear_cache_refusal: None,
            eviction_refusal: None,
            cache_bytes: 4_000_000_000,
        }
    }

    #[test]
    fn observed_idle_matches_wall_clock_when_continuously_alive() {
        let last = 1_000;
        let now = last + 10_000;
        let alive = intervals(&[(last, now)]);
        assert_eq!(observed_idle_secs(last, &alive, now), 10_000);
    }

    #[test]
    fn app_closed_gap_accrues_zero_observed_idle() {
        let last = 1_000;
        // Alive for 1h, then 7 days off, then alive 1h.
        let day = 86_400;
        let alive = intervals(&[
            (last, last + 3_600),
            (last + 7 * day + 3_600, last + 7 * day + 7_200),
        ]);
        let now = last + 7 * day + 7_200;
        // Only the two 1h windows overlapping [last, now] count.
        assert_eq!(observed_idle_secs(last, &alive, now), 7_200);
    }

    #[test]
    fn crash_loses_at_most_one_heartbeat_granule() {
        let last = 0;
        // Last stamp at t=60; crash; reopen at t=1000. Closed interval ends at 60.
        let alive = intervals(&[(0, 60), (1000, 1060)]);
        assert_eq!(observed_idle_secs(last, &alive, 1060), 120);
    }

    #[test]
    fn used_then_shutdown_7_days_not_idle_candidate_on_reopen() {
        let last = 1_000_000;
        let day = 86_400_i64;
        // Touched, then app closed for 7 days; just reopened (new interval starts).
        let now = last + 7 * day;
        let alive = intervals(&[(last - 3_600, last + 60), (now, now + 30)]);
        let observed = observed_idle_secs(last, &alive, now + 30);
        // Only ~60s before close + 30s after reopen.
        assert!(observed < 120);
        let class = classify_reclaim_candidate(&base_input(
            Some(last),
            &alive,
            now + 30,
            PrLinkState::None,
        ));
        assert!(!class.candidate);
    }

    #[test]
    fn storm_test_n_worktrees_zero_idle_only_candidates_merged_unaffected() {
        let last = 5_000_000;
        let day = 86_400_i64;
        let now = last + 7 * day;
        let alive = intervals(&[(last - 100, last + 30), (now, now + 10)]);
        let mut idle_only = 0;
        let mut merged = 0;
        for i in 0..12 {
            let pr = if i < 3 {
                PrLinkState::Merged { number: 100 + i }
            } else {
                PrLinkState::None
            };
            let class = classify_reclaim_candidate(&base_input(Some(last), &alive, now + 10, pr));
            if class.candidate {
                if matches!(pr, PrLinkState::Merged { .. }) {
                    merged += 1;
                } else {
                    idle_only += 1;
                }
            }
        }
        assert_eq!(idle_only, 0, "post-absence storm must be impossible");
        assert_eq!(merged, 3, "merged-PR candidates remain absence-independent");
    }

    #[test]
    fn idle_by_observed_time_yields_lean_when_unpushed() {
        let last = 0;
        let now = DEFAULT_IDLE_THRESHOLD_SECS + 3_600;
        let alive = intervals(&[(0, now)]);
        let class =
            classify_reclaim_candidate(&base_input(Some(last), &alive, now, PrLinkState::None));
        assert!(class.candidate);
        assert_eq!(class.tier, Some(ReclaimTier::Lean));
        assert!(class.reason.contains("observed"));
    }

    #[test]
    fn merged_pr_clean_yields_hibernate() {
        let alive = intervals(&[(0, 100)]);
        let class = classify_reclaim_candidate(&base_input(
            Some(50),
            &alive,
            100,
            PrLinkState::Merged { number: 167 },
        ));
        assert_eq!(class.tier, Some(ReclaimTier::Hibernate));
        assert!(class.reason.contains("PR #167 merged"));
    }

    #[test]
    fn closed_unmerged_pr_is_not_merged() {
        let long = intervals(&[(0, DEFAULT_IDLE_THRESHOLD_SECS + 10)]);
        // Idle qualifies → Lean (not Hibernate; unpushed + not merged).
        let class = classify_reclaim_candidate(&base_input(
            Some(0),
            &long,
            DEFAULT_IDLE_THRESHOLD_SECS + 10,
            PrLinkState::ClosedUnmerged { number: 9 },
        ));
        assert_eq!(class.tier, Some(ReclaimTier::Lean));

        // Below idle threshold → not a candidate.
        let short = intervals(&[(0, 100)]);
        let class = classify_reclaim_candidate(&base_input(
            Some(0),
            &short,
            100,
            PrLinkState::ClosedUnmerged { number: 9 },
        ));
        assert!(!class.candidate);
        assert!(class.reason.contains("closed unmerged"));
    }

    #[test]
    fn no_pr_pushed_idle_yields_hibernate() {
        let now = DEFAULT_IDLE_THRESHOLD_SECS + 10;
        let alive = intervals(&[(0, now)]);
        let mut input = base_input(Some(0), &alive, now, PrLinkState::None);
        input.branch_pushed = true;
        let class = classify_reclaim_candidate(&input);
        assert_eq!(class.tier, Some(ReclaimTier::Hibernate));
        assert!(class.reason.contains("pushed"));
    }

    #[test]
    fn no_pr_unpushed_idle_yields_lean_only() {
        let now = DEFAULT_IDLE_THRESHOLD_SECS + 10;
        let alive = intervals(&[(0, now)]);
        let input = base_input(Some(0), &alive, now, PrLinkState::None);
        let class = classify_reclaim_candidate(&input);
        assert_eq!(class.tier, Some(ReclaimTier::Lean));
    }

    #[test]
    fn dirty_blocks_hibernate_allows_lean() {
        let alive = intervals(&[(0, 100)]);
        let mut input = base_input(Some(0), &alive, 100, PrLinkState::Merged { number: 1 });
        input.dirty = true;
        input.can_evict = false;
        input.eviction_refusal = Some("dirty");
        let class = classify_reclaim_candidate(&input);
        assert_eq!(class.tier, Some(ReclaimTier::Lean));
    }

    #[test]
    fn active_lease_is_read_only_non_candidate() {
        let alive = intervals(&[(0, DEFAULT_IDLE_THRESHOLD_SECS + 10)]);
        let mut input = base_input(
            Some(0),
            &alive,
            DEFAULT_IDLE_THRESHOLD_SECS + 10,
            PrLinkState::Merged { number: 1 },
        );
        input.busy = true;
        input.can_clear_cache = false;
        input.can_evict = false;
        let class = classify_reclaim_candidate(&input);
        assert!(!class.candidate);
        assert!(class.read_only);
        assert!(class.reason.contains("busy"));
    }

    #[test]
    fn pr_link_state_prefers_merged_over_closed() {
        assert_eq!(
            pr_link_state(&[(1, "CLOSED"), (2, "MERGED")]),
            PrLinkState::Merged { number: 2 }
        );
        assert_eq!(
            pr_link_state(&[(3, "CLOSED")]),
            PrLinkState::ClosedUnmerged { number: 3 }
        );
        assert_eq!(
            pr_link_state(&[(4, "OPEN")]),
            PrLinkState::Open { number: 4 }
        );
        assert_eq!(pr_link_state(&[]), PrLinkState::None);
    }
}
