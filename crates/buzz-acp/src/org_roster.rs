//! Org roster fetch/cache and kickoff gate (Crew).
//!
//! #233 / D-069: Org chart / tree budgets / ORG-CHECK are **not** Crew product.
//! `KIND_ORG_ROSTER` may still land on the relay for sync; this module keeps
//! parse/cache helpers but does not teach the tree or enforce budgets.
//!
//! Ordinary wakes must not fetch the roster. Fetch may still happen on
//! `crew-handoff` targeting this agent or inbound `KIND_ORG_ROSTER` (cache
//! refresh) so protocol leftovers stay consistent — agents are not prompted
//! about them.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use buzz_core::kind::KIND_ORG_ROSTER;
use buzz_core::org_roster::{
    handoff_creates_work, parse_handoff_tag, parse_org_roster_event, OrgRoster, CREW_BUDGET_STOP,
    CREW_BUDGET_TAG, ORG_ROSTER_D_TAG,
};
use nostr::{Event, EventBuilder, Filter, Kind, Tag};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::relay::RestClient;

const ROSTER_FETCH_TIMEOUT: Duration = Duration::from_secs(3);

/// Cached founder-signed roster plus the event coordinates used for LWW.
#[derive(Debug, Clone)]
pub struct CachedOrgRoster {
    /// Parsed tree.
    pub roster: OrgRoster,
    /// Event id of the cached roster.
    pub event_id: String,
    /// `created_at` of the cached roster.
    pub created_at: u64,
}

/// Community-global roster cache (not per-channel).
pub type OrgRosterCache = Arc<RwLock<Option<CachedOrgRoster>>>;

/// Self-initiated work counters for turn-start budget.
#[derive(Debug)]
pub struct OrgBudgetTracker {
    utc_day: AtomicU64,
    tokens_today: AtomicU64,
    open_self_initiated: AtomicU32,
    last_heartbeat_activity: AtomicU64,
    /// Unix seconds of last inbound event seen by this harness.
    pub last_inbound_at: AtomicU64,
}

impl Default for OrgBudgetTracker {
    fn default() -> Self {
        Self {
            utc_day: AtomicU64::new(0),
            tokens_today: AtomicU64::new(0),
            open_self_initiated: AtomicU32::new(0),
            last_heartbeat_activity: AtomicU64::new(0),
            last_inbound_at: AtomicU64::new(0),
        }
    }
}

impl OrgBudgetTracker {
    fn roll_day(&self, now: u64) {
        let day = now / 86_400;
        let prev = self.utc_day.load(Ordering::Relaxed);
        if prev != day {
            self.utc_day.store(day, Ordering::Relaxed);
            self.tokens_today.store(0, Ordering::Relaxed);
        }
    }

    /// Record tokens after a completed self-initiated turn.
    pub fn record_self_initiated_tokens(&self, tokens: u64, now: u64) {
        self.roll_day(now);
        self.tokens_today.fetch_add(tokens, Ordering::Relaxed);
    }

    /// Record tokens for a finished self-initiated turn.
    ///
    /// Skips `:initial` session-setup metrics and assigned-work turns (those
    /// never acquire [`SelfInitiatedTurnGuard`], so `open_self_initiated` is 0).
    pub fn record_self_initiated_turn_tokens(&self, tokens: u64, turn_id: &str, now: u64) {
        if turn_id.contains(":initial") {
            return;
        }
        if self.open_self_initiated.load(Ordering::Relaxed) == 0 {
            return;
        }
        self.record_self_initiated_tokens(tokens, now);
    }

    /// Close one self-initiated in-flight slot.
    pub fn close_self_initiated(&self) {
        let _ =
            self.open_self_initiated
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    Some(value.saturating_sub(1))
                });
    }

    /// Snapshot for stop-and-report copy.
    pub fn queued_summary(&self, now: u64) -> (u64, u32) {
        self.roll_day(now);
        (
            self.tokens_today.load(Ordering::Relaxed),
            self.open_self_initiated.load(Ordering::Relaxed),
        )
    }
}

/// Empty cache for PromptContext construction.
pub fn empty_roster_cache() -> OrgRosterCache {
    Arc::new(RwLock::new(None))
}

fn tag_vecs(event: &Event) -> Vec<Vec<String>> {
    event
        .tags
        .iter()
        .map(|tag| tag.as_slice().to_vec())
        .collect()
}

/// Pick the newest valid roster event from a `/query` JSON array.
pub fn roster_from_query_events(
    events: &[Value],
    _expected_founder: Option<&str>,
) -> Option<CachedOrgRoster> {
    let mut best: Option<CachedOrgRoster> = None;
    for value in events {
        let event: Event = serde_json::from_value(value.clone()).ok()?;
        if u32::from(event.kind.as_u16()) != KIND_ORG_ROSTER {
            continue;
        }
        let tags = tag_vecs(&event);
        let author = event.pubkey.to_hex();
        let Ok(mut roster) = parse_org_roster_event(&event.content, &author, &tags) else {
            continue;
        };
        // Ingest already required founder-signed; cache uses the event author.
        roster.event_id = Some(event.id.to_hex());
        roster.created_at = Some(event.created_at.as_secs());
        let candidate = CachedOrgRoster {
            roster,
            event_id: event.id.to_hex(),
            created_at: event.created_at.as_secs(),
        };
        let replace = match &best {
            None => true,
            Some(current) => {
                candidate.created_at > current.created_at
                    || (candidate.created_at == current.created_at
                        && candidate.event_id > current.event_id)
            }
        };
        if replace {
            best = Some(candidate);
        }
    }
    best
}

/// Store `incoming` when it is newer than the cache (reorg race: newer wins).
pub async fn cache_if_newer(cache: &OrgRosterCache, incoming: CachedOrgRoster) {
    let mut guard = cache.write().await;
    let replace = match guard.as_ref() {
        None => true,
        Some(current) => {
            incoming.created_at > current.created_at
                || (incoming.created_at == current.created_at
                    && incoming.event_id > current.event_id)
        }
    };
    if replace {
        *guard = Some(incoming);
    }
}

/// Refresh cache from an inbound roster event. In-flight turns keep the clone
/// they already read; the next turn sees the new tree.
pub async fn cache_inbound_roster(cache: &OrgRosterCache, event: &Event) {
    let tags = tag_vecs(event);
    let author = event.pubkey.to_hex();
    let Ok(mut roster) = parse_org_roster_event(&event.content, &author, &tags) else {
        return;
    };
    roster.event_id = Some(event.id.to_hex());
    roster.created_at = Some(event.created_at.as_secs());
    cache_if_newer(
        cache,
        CachedOrgRoster {
            roster,
            event_id: event.id.to_hex(),
            created_at: event.created_at.as_secs(),
        },
    )
    .await;
}

/// Fetch roster from the relay when the cache is empty. Does not refresh a
/// populated cache (ordinary path). Handoff/session callers use this.
pub async fn cached_or_fetch_roster(
    cache: &OrgRosterCache,
    rest: &RestClient,
    founder: Option<&str>,
) -> Option<OrgRoster> {
    if let Some(cached) = cache.read().await.as_ref() {
        return Some(cached.roster.clone());
    }
    fetch_and_store_roster(cache, rest, founder).await
}

async fn fetch_and_store_roster(
    cache: &OrgRosterCache,
    rest: &RestClient,
    founder: Option<&str>,
) -> Option<OrgRoster> {
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_ORG_ROSTER as u16))
        .identifier(ORG_ROSTER_D_TAG.to_string())
        .limit(5);
    let json = match tokio::time::timeout(
        ROSTER_FETCH_TIMEOUT,
        rest.query(std::slice::from_ref(&filter)),
    )
    .await
    {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            tracing::warn!(error = %error, "org roster query failed");
            return None;
        }
        Err(_) => {
            tracing::warn!("org roster fetch timed out");
            return None;
        }
    };
    let events = json.as_array()?;
    let cached = roster_from_query_events(events, founder)?;
    let roster = cached.roster.clone();
    cache_if_newer(cache, cached).await;
    Some(roster)
}

/// Kickoff gate: fetch roster only when this agent is the handoff executor.
///
/// Returns `true` when the inbound event should start a turn. Peer handoffs
/// return `false` (conversation only — do not drop the relay event).
pub async fn handoff_should_create_work(
    cache: &OrgRosterCache,
    rest: &RestClient,
    author: &str,
    executor: &str,
    founder: &str,
    me: &str,
) -> bool {
    let Ok(executor) = buzz_core::org_roster::canonical_pubkey(executor) else {
        return false;
    };
    let Ok(me) = buzz_core::org_roster::canonical_pubkey(me) else {
        return false;
    };
    if executor != me {
        return true;
    }
    let roster = cached_or_fetch_roster(cache, rest, Some(founder)).await;
    handoff_creates_work(roster.as_ref(), author, &executor, founder)
}

/// Whether this batch is founder/manager-assigned work (never budget-blocked).
pub fn batch_is_assigned_work<'a, I>(
    events: I,
    me: &str,
    roster: Option<&OrgRoster>,
    founder: &str,
) -> bool
where
    I: IntoIterator<Item = &'a Event>,
{
    for event in events {
        let tags = tag_vecs(event);
        let Some(handoff) = parse_handoff_tag(&tags) else {
            continue;
        };
        if handoff_creates_work(roster, &event.pubkey.to_hex(), &handoff.executor, founder)
            && canonical_eq(&handoff.executor, me)
        {
            return true;
        }
    }
    false
}

fn canonical_eq(left: &str, right: &str) -> bool {
    buzz_core::org_roster::canonical_pubkey(left)
        .ok()
        .zip(buzz_core::org_roster::canonical_pubkey(right).ok())
        .is_some_and(|(a, b)| a == b)
}

/// Budget decision at turn start.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // StopAndReport kept for protocol shape / possible P3
pub enum BudgetDecision {
    /// Run the turn.
    Allow,
    /// Self-initiated work hit the ceiling — stop-and-report, skip the turn.
    StopAndReport {
        /// Tokens used today (self-initiated).
        tokens_today: u64,
        /// Concurrent self-initiated slots including this attempt.
        open_work: u32,
        /// Office channel for the report, if the roster named one.
        office_channel: Option<String>,
    },
}

/// Evaluate self-initiated budget. Assigned work always `Allow`.
///
/// #233 / D-069: tree budgets are out of product — always allow.
pub fn evaluate_budget(
    tracker: &OrgBudgetTracker,
    roster: Option<&OrgRoster>,
    me: &str,
    assigned_work: bool,
    now: u64,
) -> BudgetDecision {
    let _ = (tracker, roster, me, assigned_work, now);
    BudgetDecision::Allow
}

/// RAII slot for one self-initiated in-flight turn.
pub struct SelfInitiatedTurnGuard {
    tracker: Arc<OrgBudgetTracker>,
}

impl SelfInitiatedTurnGuard {
    /// Increment the open-work counter for the lifetime of this turn.
    pub fn acquire(tracker: Arc<OrgBudgetTracker>) -> Self {
        tracker.open_self_initiated.fetch_add(1, Ordering::Relaxed);
        Self { tracker }
    }
}

impl Drop for SelfInitiatedTurnGuard {
    fn drop(&mut self) {
        self.tracker.close_self_initiated();
    }
}

/// Officers skip heartbeat when nothing new arrived since the last one.
///
/// #233 / D-069: officer loop is not product — never skip on roster grounds.
pub fn officer_should_skip_heartbeat(
    roster: Option<&OrgRoster>,
    me: &str,
    tracker: &OrgBudgetTracker,
) -> bool {
    let _ = (roster, me, tracker);
    false
}

/// Mark a heartbeat as fired (delta gate baseline).
pub fn mark_heartbeat_fired(tracker: &OrgBudgetTracker, now: u64) {
    tracker
        .last_heartbeat_activity
        .store(now, Ordering::Relaxed);
}

/// ORG-CHECK prompt section for a fresh session.
///
/// #233 / D-069: Org is not Crew product. Keep the roster parser for sync, but
/// never inject ORG-CHECK / tree teaching into agent prompts.
pub fn org_check_section(roster: Option<&OrgRoster>, me: &str) -> Option<String> {
    let _ = (roster, me);
    None
}

/// Publish stop-and-report into the officer office channel (kind:9).
pub async fn publish_stop_and_report(
    rest: &RestClient,
    keys: &nostr::Keys,
    office_channel: &str,
    tokens_today: u64,
    open_work: u32,
) -> Result<(), String> {
    let content = format!(
        "Budget reached · {open_work} items queued (self-initiated tokens today: {tokens_today}). Founder-assigned work is not blocked."
    );
    let channel = office_channel.trim();
    uuid::Uuid::parse_str(channel).map_err(|_| "office_channel is not a UUID".to_string())?;
    let h_tag =
        Tag::parse(["h", channel]).map_err(|error| format!("invalid office h tag: {error}"))?;
    let budget_tag = Tag::parse([CREW_BUDGET_TAG, CREW_BUDGET_STOP])
        .map_err(|error| format!("invalid budget tag: {error}"))?;
    let event = EventBuilder::new(Kind::Custom(9), content)
        .tags([h_tag, budget_tag])
        .sign_with_keys(keys)
        .map_err(|error| format!("sign stop-and-report: {error}"))?;
    rest.submit_event(&event)
        .await
        .map_err(|error| format!("publish stop-and-report: {error}"))?;
    Ok(())
}

/// Parse a handoff off a live nostr event.
pub fn parse_event_handoff(event: &Event) -> Option<buzz_core::org_roster::CrewHandoff> {
    parse_handoff_tag(&tag_vecs(event))
}

#[cfg(test)]
mod tests {
    use super::*;
    use buzz_core::org_roster::{parse_org_roster, serialize_org_roster};
    use nostr::{EventBuilder, Keys, Kind, Tag};
    use std::sync::Arc;

    fn fixture() -> (String, String, String, String, OrgRoster) {
        let founder = Keys::generate().public_key().to_hex();
        let hermes = Keys::generate().public_key().to_hex();
        let cody = Keys::generate().public_key().to_hex();
        let peer = Keys::generate().public_key().to_hex();
        let json = format!(
            r#"{{"nodes":{{"{hermes}":{{"manager":"{founder}","domain":"eng","budget":{{"tokens_per_day":80,"open_work_cap":2}}}},"{cody}":{{"manager":"{hermes}","domain":"impl","budget":{{"tokens_per_day":40,"open_work_cap":1}}}}}}}}"#
        );
        let roster = parse_org_roster(&json, &founder).expect("roster");
        (founder, hermes, cody, peer, roster)
    }

    #[test]
    fn gate_matrix() {
        let (founder, hermes, cody, peer, roster) = fixture();
        assert!(handoff_creates_work(
            Some(&roster),
            &hermes,
            &cody,
            &founder
        ));
        assert!(handoff_creates_work(
            Some(&roster),
            &founder,
            &cody,
            &founder
        ));
        assert!(handoff_creates_work(
            Some(&roster),
            &founder,
            &hermes,
            &founder
        ));
        assert!(!handoff_creates_work(Some(&roster), &peer, &cody, &founder));
    }

    #[test]
    fn founder_assigned_never_budget_blocked() {
        let (_founder, hermes, _cody, _peer, roster) = fixture();
        let tracker = OrgBudgetTracker::default();
        tracker.tokens_today.store(10_000, Ordering::Relaxed);
        tracker.open_self_initiated.store(9, Ordering::Relaxed);
        assert_eq!(
            evaluate_budget(&tracker, Some(&roster), &hermes, true, 0),
            BudgetDecision::Allow
        );
    }

    #[test]
    fn self_initiated_budget_inert_always_allows() {
        let (_founder, hermes, _cody, _peer, roster) = fixture();
        let tracker = OrgBudgetTracker::default();
        tracker.tokens_today.store(80, Ordering::Relaxed);
        let decision = evaluate_budget(&tracker, Some(&roster), &hermes, false, 0);
        assert_eq!(decision, BudgetDecision::Allow);
    }

    #[test]
    fn open_work_cap_inert_always_allows() {
        let (_founder, _hermes, cody, _peer, roster) = fixture();
        let tracker = OrgBudgetTracker::default();
        tracker.open_self_initiated.store(1, Ordering::Relaxed);
        let decision = evaluate_budget(&tracker, Some(&roster), &cody, false, 0);
        assert_eq!(decision, BudgetDecision::Allow);
    }

    #[test]
    fn newer_roster_wins_reorg_race() {
        let (_founder, hermes, cody, _peer, roster) = fixture();
        let older = CachedOrgRoster {
            roster: roster.clone(),
            event_id: "aa".repeat(32),
            created_at: 10,
        };
        let mut newer_roster = roster;
        newer_roster.nodes.remove(&cody);
        let newer = CachedOrgRoster {
            roster: newer_roster,
            event_id: "bb".repeat(32),
            created_at: 11,
        };
        assert!(newer.created_at > older.created_at);
        assert!(!newer.roster.nodes.contains_key(&cody));
        assert!(older.roster.nodes.contains_key(&hermes));
    }

    #[test]
    fn officer_heartbeat_delta_gate_inert() {
        let (_founder, hermes, cody, _peer, roster) = fixture();
        let tracker = OrgBudgetTracker::default();
        tracker.last_heartbeat_activity.store(50, Ordering::Relaxed);
        tracker.last_inbound_at.store(40, Ordering::Relaxed);
        assert!(!officer_should_skip_heartbeat(
            Some(&roster),
            &hermes,
            &tracker
        ));
        assert!(!officer_should_skip_heartbeat(
            Some(&roster),
            &cody,
            &tracker
        ));
    }

    #[test]
    fn serialize_round_trip_stable() {
        let (founder, _hermes, _cody, _peer, roster) = fixture();
        let json = serialize_org_roster(&roster).unwrap();
        let again = parse_org_roster(&json, &founder).unwrap();
        assert_eq!(roster.nodes, again.nodes);
    }

    #[test]
    fn assigned_work_uses_manager_chain() {
        let (founder, _hermes, cody, _peer, roster) = fixture();
        assert!(!batch_is_assigned_work(
            std::iter::empty(),
            &cody,
            Some(&roster),
            &founder
        ));
    }

    #[test]
    fn parent_link_and_token_accounting() {
        let keys = Keys::generate();
        let parent = "ab".repeat(32);
        let event = EventBuilder::new(Kind::Custom(9), "hi")
            .tag(Tag::parse(["e", parent.as_str()]).expect("e tag"))
            .sign_with_keys(&keys)
            .expect("sign");
        assert!(buzz_core::org_roster::has_parent_e_tag(&tag_vecs(&event)));
        let tracker = Arc::new(OrgBudgetTracker::default());
        let _guard = SelfInitiatedTurnGuard::acquire(Arc::clone(&tracker));
        tracker.record_self_initiated_turn_tokens(12, "turn-1", 1);
        assert_eq!(tracker.queued_summary(1), (12, 1));
        tracker.record_self_initiated_turn_tokens(5, "turn-1:initial", 1);
        assert_eq!(tracker.queued_summary(1).0, 12);
    }
}
