//! Founder-signed community org roster (Crew).
//!
//! Storage is one addressable event ([`crate::kind::KIND_ORG_ROSTER`]) whose
//! `d` tag is the well-known [`ORG_ROSTER_D_TAG`]. The community boundary is
//! the relay tenant; clients never need the internal community UUID. The
//! org chart is a projection of this event plus maintainer/steward tags on
//! 30617/30621 — never a parallel registry.
//!
//! Rejected alternatives (see DECISIONS.md): per-agent `manager` tags on
//! 30177, and an `org:` section in the home-channel canvas.

use std::collections::{BTreeMap, HashSet};

use nostr::{FromBech32, PublicKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// NIP-33 `d` tag for the community org roster.
pub const ORG_ROSTER_D_TAG: &str = "org";
/// Tolerant handoff tag: `["crew-handoff", <executor-pubkey>, <goal-digest>]`.
pub const CREW_HANDOFF_TAG: &str = "crew-handoff";
/// Tolerant stop-and-report tag: `["crew-budget", "stop"]`.
pub const CREW_BUDGET_TAG: &str = "crew-budget";
/// Budget-stop tag value.
pub const CREW_BUDGET_STOP: &str = "stop";
/// Office-thread marker: `["crew-office", <officer-pubkey>]`.
pub const CREW_OFFICE_TAG: &str = "crew-office";
/// NIP-34 maintainer tag name (stewardship).
pub const MAINTAINERS_TAG: &str = "maintainers";
/// Crew steward alias on 30617/30621 (same meaning as maintainers).
pub const STEWARD_TAG: &str = "steward";

const MAX_DOMAIN_LEN: usize = 128;
const MAX_DUTIES_LEN: usize = 4096;
const MAX_CADENCE_LEN: usize = 128;
const MAX_NODES: usize = 256;

/// Tokens/day + concurrent self-initiated work cap for one agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgBudget {
    /// Self-initiated token budget per UTC day. Founder-assigned work is
    /// never charged against this.
    pub tokens_per_day: u64,
    /// Cap on concurrently open self-initiated work (D-005 spirit, per agent).
    pub open_work_cap: u32,
}

/// One agent in the founder-signed tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgNode {
    /// Canonical lowercase hex pubkey.
    pub agent_pubkey: String,
    /// Manager pubkey (founder or another roster node).
    pub manager: String,
    /// Free-form domain label (not a hard-coded title — D-030).
    pub domain: String,
    /// Founder-authored duties text.
    pub duties: String,
    /// Reporting cadence convention (not protocol-enforced).
    pub cadence: String,
    /// Self-initiated work budget.
    pub budget: OrgBudget,
    /// Optional office-channel UUID for rollups and stop-and-report.
    pub office_channel: Option<String>,
}

/// Parsed founder-signed roster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgRoster {
    /// Agent pubkey → node. Founder is the tree root and is not a node.
    pub nodes: BTreeMap<String, OrgNode>,
    /// Founder pubkey that signed (or will sign) the roster.
    pub founder_pubkey: String,
    /// Addressable event id when loaded from the relay.
    pub event_id: Option<String>,
    /// `created_at` of the roster event, for reorg-race cache compare.
    pub created_at: Option<u64>,
}

/// Parsed `["crew-handoff", executor, goal-digest]` tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrewHandoff {
    /// Executor pubkey (canonical hex).
    pub executor: String,
    /// SHA-256 hex of the kickoff goal text.
    pub goal_digest: String,
}

/// Errors that make a roster fail closed.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum OrgRosterError {
    /// Content is not a JSON object of the supported shape.
    #[error("invalid org roster JSON: {0}")]
    InvalidJson(String),
    /// A pubkey is not a valid Nostr key.
    #[error("invalid org roster pubkey: {0}")]
    InvalidPubkey(String),
    /// Domain / duties / cadence failed format-only checks.
    #[error("invalid org roster field: {0}")]
    InvalidField(String),
    /// Reporting graph contains a cycle.
    #[error("org roster cycle: {0}")]
    Cycle(String),
    /// A manager is neither the founder nor a roster node.
    #[error("org roster orphan: {0} reports to unknown manager {1}")]
    Orphan(String, String),
    /// An agent lists itself as manager.
    #[error("org roster self-manager: {0}")]
    SelfManager(String),
    /// Founder cannot appear as an employee node.
    #[error("org roster founder cannot be a node")]
    FounderAsNode,
    /// Child budget exceeds parent budget.
    #[error("org roster budget overflow: {0} exceeds manager {1}")]
    BudgetOverflow(String, String),
    /// Too many nodes.
    #[error("org roster exceeds {MAX_NODES} nodes")]
    TooManyNodes,
    /// An agent is not a known community member.
    #[error("org roster unknown agent: {0}")]
    UnknownAgent(String),
    /// Event `d` tag is missing or not [`ORG_ROSTER_D_TAG`].
    #[error("org roster d-tag must be `{ORG_ROSTER_D_TAG}`")]
    InvalidDTag,
    /// Author is not the community founder.
    #[error("org roster must be founder-signed")]
    NonFounder,
    /// Sub-kickoff is missing its parent thread link.
    #[error("sub-kickoff requires a parent thread link")]
    MissingParentLink,
}

#[derive(Debug, Deserialize)]
struct RawRoster {
    #[serde(default)]
    nodes: BTreeMap<String, RawNode>,
}

#[derive(Debug, Deserialize)]
struct RawNode {
    manager: String,
    #[serde(default)]
    domain: String,
    #[serde(default)]
    duties: String,
    #[serde(default)]
    cadence: String,
    budget: RawBudget,
    #[serde(default)]
    office_channel: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawBudget {
    tokens_per_day: u64,
    open_work_cap: u32,
}

/// SHA-256 hex digest of kickoff goal text (anti-telephone).
pub fn goal_digest(goal: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(goal.as_bytes());
    hex::encode(hasher.finalize())
}

/// Parse roster JSON. Unknown keys are ignored (forward compatible).
///
/// `founder_pubkey` is the expected signer. Tree validation runs here so a
/// broken roster never exists as a parsed value.
pub fn parse_org_roster(content: &str, founder_pubkey: &str) -> Result<OrgRoster, OrgRosterError> {
    let founder = canonical_pubkey(founder_pubkey)?;
    let raw: RawRoster =
        serde_json::from_str(content).map_err(|e| OrgRosterError::InvalidJson(e.to_string()))?;
    if raw.nodes.len() > MAX_NODES {
        return Err(OrgRosterError::TooManyNodes);
    }
    let mut nodes = BTreeMap::new();
    for (agent, raw_node) in raw.nodes {
        let agent = canonical_pubkey(&agent)?;
        if agent == founder {
            return Err(OrgRosterError::FounderAsNode);
        }
        let manager = canonical_pubkey(&raw_node.manager)?;
        if manager == agent {
            return Err(OrgRosterError::SelfManager(agent));
        }
        let domain = normalize_label(&raw_node.domain, MAX_DOMAIN_LEN, "domain")?;
        if domain.is_empty() {
            return Err(OrgRosterError::InvalidField("domain is empty".into()));
        }
        let duties = normalize_text(&raw_node.duties, MAX_DUTIES_LEN, "duties")?;
        let cadence = normalize_label(&raw_node.cadence, MAX_CADENCE_LEN, "cadence")?;
        if let Some(channel) = raw_node.office_channel.as_deref() {
            uuid::Uuid::parse_str(channel.trim()).map_err(|_| {
                OrgRosterError::InvalidField(format!("office_channel is not a UUID: {channel}"))
            })?;
        }
        nodes.insert(
            agent.clone(),
            OrgNode {
                agent_pubkey: agent,
                manager,
                domain,
                duties,
                cadence,
                budget: OrgBudget {
                    tokens_per_day: raw_node.budget.tokens_per_day,
                    open_work_cap: raw_node.budget.open_work_cap,
                },
                office_channel: raw_node
                    .office_channel
                    .map(|value| value.trim().to_string()),
            },
        );
    }
    let roster = OrgRoster {
        nodes,
        founder_pubkey: founder,
        event_id: None,
        created_at: None,
    };
    validate_tree(&roster)?;
    Ok(roster)
}

/// Structural tree checks: orphans, cycles, budget cascade.
pub fn validate_tree(roster: &OrgRoster) -> Result<(), OrgRosterError> {
    let founder = &roster.founder_pubkey;
    for node in roster.nodes.values() {
        if node.manager != *founder && !roster.nodes.contains_key(&node.manager) {
            return Err(OrgRosterError::Orphan(
                node.agent_pubkey.clone(),
                node.manager.clone(),
            ));
        }
        if node.manager != *founder {
            let Some(parent) = roster.nodes.get(&node.manager) else {
                return Err(OrgRosterError::Orphan(
                    node.agent_pubkey.clone(),
                    node.manager.clone(),
                ));
            };
            if node.budget.tokens_per_day > parent.budget.tokens_per_day
                || node.budget.open_work_cap > parent.budget.open_work_cap
            {
                return Err(OrgRosterError::BudgetOverflow(
                    node.agent_pubkey.clone(),
                    node.manager.clone(),
                ));
            }
        }
        detect_cycle(roster, &node.agent_pubkey)?;
    }
    Ok(())
}

/// Reject agents that are not in `known_members` (relay members / managed agents).
pub fn reject_unknown_agents(
    roster: &OrgRoster,
    known_members: &HashSet<String>,
) -> Result<(), OrgRosterError> {
    for agent in roster.nodes.keys() {
        if !known_members.contains(agent) {
            return Err(OrgRosterError::UnknownAgent(agent.clone()));
        }
    }
    Ok(())
}

/// Walk manager pointers from `agent` up to the founder (inclusive of founder).
pub fn manager_chain(roster: &OrgRoster, agent: &str) -> Result<Vec<String>, OrgRosterError> {
    let agent = canonical_pubkey(agent)?;
    let mut chain = Vec::new();
    let mut cursor = agent;
    let mut seen = HashSet::new();
    loop {
        if cursor == roster.founder_pubkey {
            chain.push(cursor);
            return Ok(chain);
        }
        if !seen.insert(cursor.clone()) {
            return Err(OrgRosterError::Cycle(cursor));
        }
        let Some(node) = roster.nodes.get(&cursor) else {
            return Err(OrgRosterError::Orphan(cursor.clone(), String::new()));
        };
        chain.push(cursor.clone());
        cursor = node.manager.clone();
    }
}

/// `true` when `author` is the founder or an ancestor of `executor`.
pub fn author_in_manager_chain(
    roster: &OrgRoster,
    author: &str,
    executor: &str,
) -> Result<bool, OrgRosterError> {
    let author = canonical_pubkey(author)?;
    if author == roster.founder_pubkey {
        return Ok(true);
    }
    let chain = manager_chain(roster, executor)?;
    Ok(chain.iter().any(|pubkey| pubkey == &author))
}

/// Kickoff with a handoff tag creates work only when the author is in the
/// executor's manager chain (or is the founder). Peers degrade to conversation.
pub fn handoff_creates_work(
    roster: Option<&OrgRoster>,
    author: &str,
    executor: &str,
    founder: &str,
) -> bool {
    let Ok(author) = canonical_pubkey(author) else {
        return false;
    };
    let Ok(founder) = canonical_pubkey(founder) else {
        return false;
    };
    if author == founder {
        return true;
    }
    let Some(roster) = roster else {
        return false;
    };
    author_in_manager_chain(roster, &author, executor).unwrap_or(false)
}

/// Parse the first `crew-handoff` tag. Unknown extra fields after digest are ignored.
pub fn parse_handoff_tag(tags: &[Vec<String>]) -> Option<CrewHandoff> {
    for tag in tags {
        if tag.first().map(String::as_str) != Some(CREW_HANDOFF_TAG) {
            continue;
        }
        let executor = tag.get(1)?;
        let digest = tag.get(2)?;
        if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let Ok(executor) = canonical_pubkey(executor) else {
            continue;
        };
        return Some(CrewHandoff {
            executor,
            goal_digest: digest.to_ascii_lowercase(),
        });
    }
    None
}

/// True when tags include a NIP-10 `e` parent/root marker with a 64-hex id.
pub fn has_parent_e_tag(tags: &[Vec<String>]) -> bool {
    tags.iter().any(|tag| {
        tag.first().map(String::as_str) == Some("e")
            && tag
                .get(1)
                .is_some_and(|id| id.len() == 64 && id.chars().all(|c| c.is_ascii_hexdigit()))
    })
}

/// First `d` tag value, if any.
pub fn d_tag_value(tags: &[Vec<String>]) -> Option<&str> {
    tags.iter().find_map(|tag| {
        (tag.first().map(String::as_str) == Some("d")).then(|| tag.get(1).map(String::as_str))?
    })
}

/// Require exactly one `d` tag equal to [`ORG_ROSTER_D_TAG`].
pub fn require_org_d_tag(tags: &[Vec<String>]) -> Result<(), OrgRosterError> {
    let d_tags: Vec<&str> = tags
        .iter()
        .filter(|tag| tag.first().map(String::as_str) == Some("d"))
        .filter_map(|tag| tag.get(1).map(String::as_str))
        .collect();
    if d_tags.len() == 1 && d_tags[0] == ORG_ROSTER_D_TAG {
        Ok(())
    } else {
        Err(OrgRosterError::InvalidDTag)
    }
}

/// Parse a roster event: `d=org` plus JSON body signed by `author`.
pub fn parse_org_roster_event(
    content: &str,
    author: &str,
    tags: &[Vec<String>],
) -> Result<OrgRoster, OrgRosterError> {
    require_org_d_tag(tags)?;
    parse_org_roster(content, author)
}

/// Sub-kickoffs (non-founder authors) must carry a NIP-10 parent `e` tag.
pub fn require_parent_link_for_sub_kickoff(
    author: &str,
    founder: &str,
    has_parent_e_tag: bool,
) -> Result<(), OrgRosterError> {
    let author = canonical_pubkey(author)?;
    let founder = canonical_pubkey(founder)?;
    if author != founder && !has_parent_e_tag {
        return Err(OrgRosterError::MissingParentLink);
    }
    Ok(())
}

/// Direct reports of `manager` (one hop down).
pub fn direct_reports<'a>(
    roster: &'a OrgRoster,
    manager: &str,
) -> Result<Vec<&'a OrgNode>, OrgRosterError> {
    let manager = canonical_pubkey(manager)?;
    Ok(roster
        .nodes
        .values()
        .filter(|node| node.manager == manager)
        .collect())
}

/// `true` when the agent has at least one direct report (officer pattern).
pub fn is_officer(roster: &OrgRoster, agent: &str) -> bool {
    let Ok(agent) = canonical_pubkey(agent) else {
        return false;
    };
    roster.nodes.values().any(|node| node.manager == agent)
}

/// Collect maintainer/steward pubkeys from 30617/30621 tags.
pub fn stewardship_pubkeys(tags: &[Vec<String>]) -> Vec<String> {
    let mut out = Vec::new();
    for tag in tags {
        let Some(name) = tag.first().map(String::as_str) else {
            continue;
        };
        if name != MAINTAINERS_TAG && name != STEWARD_TAG {
            continue;
        }
        for value in tag.iter().skip(1) {
            if let Ok(pubkey) = canonical_pubkey(value) {
                if !out.contains(&pubkey) {
                    out.push(pubkey);
                }
            }
        }
    }
    out
}

/// Repositories/projects that name `agent` as maintainer or steward.
pub fn portfolio_for_agent(
    events: &[(String, Vec<Vec<String>>)],
    agent: &str,
) -> Result<Vec<String>, OrgRosterError> {
    let agent = canonical_pubkey(agent)?;
    Ok(events
        .iter()
        .filter(|(_, tags)| stewardship_pubkeys(tags).contains(&agent))
        .map(|(id, _)| id.clone())
        .collect())
}

/// Prompt section for a fresh session (ROLE-CHECK precedent).
pub fn compose_org_section(roster: &OrgRoster, agent: &str) -> Option<String> {
    let Ok(agent) = canonical_pubkey(agent) else {
        return None;
    };
    let node = roster.nodes.get(&agent)?;
    let reports = roster
        .nodes
        .values()
        .filter(|candidate| candidate.manager == agent)
        .map(|candidate| candidate.agent_pubkey.as_str())
        .collect::<Vec<_>>();
    let manager_label = if node.manager == roster.founder_pubkey {
        format!("{} (founder)", node.manager)
    } else {
        node.manager.clone()
    };
    Some(format!(
        "## Org assignment (Crew)\n\n\
         You report to: **{manager_label}**.\n\
         Domain: **{}**.\n\
         Duties: {}\n\
         Cadence (convention, not enforced): {}\n\
         Direct reports: {}.\n\
         Self-initiated budget: {} tokens/day, open-work cap {}.\n\
         Founder-assigned work is never blocked by this budget. Hitting the \
         ceiling must stop-and-report, never continue silently.\n\n\
         Peer mentions and DMs stay flat — do not route conversation through the tree.\n\
         Kickoffs that assign work use a `crew-handoff` tag; only your manager chain \
         or the founder can auto-create work for you.\n\
         Sub-kickoffs must link the parent thread (NIP-10 `e` tag) so executors can \
         read the founder's original words.\n\n\
         MANDATORY declaration: The FIRST line of your first reply on a fresh session MUST be:\n\n\
         ORG-CHECK: manager={} domain={} reports={}\n\n\
         Never omit ORG-CHECK on a fresh session.",
        node.domain,
        if node.duties.is_empty() {
            "(none)"
        } else {
            node.duties.as_str()
        },
        if node.cadence.is_empty() {
            "(none)"
        } else {
            node.cadence.as_str()
        },
        if reports.is_empty() {
            "none".to_string()
        } else {
            reports.join(", ")
        },
        node.budget.tokens_per_day,
        node.budget.open_work_cap,
        node.manager,
        node.domain,
        reports.len(),
    ))
}

/// Serialize roster JSON for founder republish (stable key order).
pub fn serialize_org_roster(roster: &OrgRoster) -> Result<String, OrgRosterError> {
    let mut nodes = serde_json::Map::new();
    for (agent, node) in &roster.nodes {
        let mut object = serde_json::Map::new();
        object.insert(
            "manager".into(),
            serde_json::Value::String(node.manager.clone()),
        );
        object.insert(
            "domain".into(),
            serde_json::Value::String(node.domain.clone()),
        );
        object.insert(
            "duties".into(),
            serde_json::Value::String(node.duties.clone()),
        );
        object.insert(
            "cadence".into(),
            serde_json::Value::String(node.cadence.clone()),
        );
        object.insert(
            "budget".into(),
            serde_json::json!({
                "tokens_per_day": node.budget.tokens_per_day,
                "open_work_cap": node.budget.open_work_cap,
            }),
        );
        if let Some(channel) = &node.office_channel {
            object.insert(
                "office_channel".into(),
                serde_json::Value::String(channel.clone()),
            );
        }
        nodes.insert(agent.clone(), serde_json::Value::Object(object));
    }
    serde_json::to_string(&serde_json::json!({ "nodes": nodes }))
        .map_err(|e| OrgRosterError::InvalidJson(e.to_string()))
}

/// Canonical lowercase hex pubkey.
pub fn canonical_pubkey(value: &str) -> Result<String, OrgRosterError> {
    let value = value.trim();
    let pk = PublicKey::from_hex(value)
        .or_else(|_| PublicKey::from_bech32(value))
        .map_err(|_| OrgRosterError::InvalidPubkey(value.to_string()))?;
    Ok(pk.to_hex().to_ascii_lowercase())
}

fn detect_cycle(roster: &OrgRoster, start: &str) -> Result<(), OrgRosterError> {
    let mut cursor = start.to_string();
    let mut seen = HashSet::new();
    loop {
        if cursor == roster.founder_pubkey {
            return Ok(());
        }
        if !seen.insert(cursor.clone()) {
            return Err(OrgRosterError::Cycle(cursor));
        }
        let Some(node) = roster.nodes.get(&cursor) else {
            return Ok(());
        };
        cursor = node.manager.clone();
    }
}

fn normalize_label(raw: &str, max: usize, field: &str) -> Result<String, OrgRosterError> {
    let label = raw.trim();
    if label.len() > max {
        return Err(OrgRosterError::InvalidField(format!(
            "{field} exceeds {max} bytes"
        )));
    }
    if label.lines().count() > 1 {
        return Err(OrgRosterError::InvalidField(format!(
            "{field} must be one line"
        )));
    }
    Ok(label.to_string())
}

fn normalize_text(raw: &str, max: usize, field: &str) -> Result<String, OrgRosterError> {
    let text = raw.trim();
    if text.len() > max {
        return Err(OrgRosterError::InvalidField(format!(
            "{field} exceeds {max} bytes"
        )));
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::Keys;

    struct Fixture {
        founder: String,
        hermes: String,
        cody: String,
        peer: String,
    }

    fn fixture() -> Fixture {
        Fixture {
            founder: Keys::generate().public_key().to_hex(),
            hermes: Keys::generate().public_key().to_hex(),
            cody: Keys::generate().public_key().to_hex(),
            peer: Keys::generate().public_key().to_hex(),
        }
    }

    fn node_json(manager: &str, domain: &str, tokens: u64, cap: u32) -> String {
        format!(
            r#"{{"manager":"{manager}","domain":"{domain}","duties":"own it","cadence":"weekly","budget":{{"tokens_per_day":{tokens},"open_work_cap":{cap}}}}}"#
        )
    }

    fn roster_json(fx: &Fixture) -> String {
        format!(
            r#"{{"nodes":{{"{}":{},"{}":{}}},"ignored":true}}"#,
            fx.hermes,
            node_json(&fx.founder, "eng", 80_000, 3),
            fx.cody,
            node_json(&fx.hermes, "impl", 40_000, 2),
        )
    }

    #[test]
    fn unknown_top_level_keys_are_ignored() {
        let fx = fixture();
        let roster = parse_org_roster(&roster_json(&fx), &fx.founder).unwrap();
        assert_eq!(roster.nodes.len(), 2);
        assert_eq!(roster.nodes[&fx.hermes].domain, "eng");
    }

    #[test]
    fn cycle_is_rejected() {
        let fx = fixture();
        let json = format!(
            r#"{{"nodes":{{"{}":{},"{}":{}}}}}"#,
            fx.hermes,
            node_json(&fx.cody, "eng", 10, 1),
            fx.cody,
            node_json(&fx.hermes, "impl", 10, 1),
        );
        assert!(matches!(
            parse_org_roster(&json, &fx.founder),
            Err(OrgRosterError::Cycle(_))
        ));
    }

    #[test]
    fn orphan_is_rejected() {
        let fx = fixture();
        let json = format!(
            r#"{{"nodes":{{"{}":{}}}}}"#,
            fx.hermes,
            node_json(&fx.peer, "eng", 10, 1),
        );
        assert!(matches!(
            parse_org_roster(&json, &fx.founder),
            Err(OrgRosterError::Orphan(_, _))
        ));
    }

    #[test]
    fn self_manager_is_rejected() {
        let fx = fixture();
        let json = format!(
            r#"{{"nodes":{{"{}":{}}}}}"#,
            fx.hermes,
            node_json(&fx.hermes, "eng", 10, 1),
        );
        assert!(matches!(
            parse_org_roster(&json, &fx.founder),
            Err(OrgRosterError::SelfManager(_))
        ));
    }

    #[test]
    fn founder_as_node_is_rejected() {
        let fx = fixture();
        let json = format!(
            r#"{{"nodes":{{"{}":{}}}}}"#,
            fx.founder,
            node_json(&fx.hermes, "eng", 10, 1),
        );
        assert!(matches!(
            parse_org_roster(&json, &fx.founder),
            Err(OrgRosterError::FounderAsNode)
        ));
    }

    #[test]
    fn budget_overflow_is_rejected() {
        let fx = fixture();
        let json = format!(
            r#"{{"nodes":{{"{}":{},"{}":{}}}}}"#,
            fx.hermes,
            node_json(&fx.founder, "eng", 10, 1),
            fx.cody,
            node_json(&fx.hermes, "impl", 11, 1),
        );
        assert!(matches!(
            parse_org_roster(&json, &fx.founder),
            Err(OrgRosterError::BudgetOverflow(_, _))
        ));
    }

    #[test]
    fn manager_array_is_rejected() {
        let fx = fixture();
        let json = format!(
            r#"{{"nodes":{{"{}":{{"manager":["{}","{}"],"domain":"eng","budget":{{"tokens_per_day":1,"open_work_cap":1}}}}}}}}"#,
            fx.hermes, fx.founder, fx.peer
        );
        assert!(matches!(
            parse_org_roster(&json, &fx.founder),
            Err(OrgRosterError::InvalidJson(_))
        ));
    }

    #[test]
    fn manager_and_grand_manager_and_founder_create_work() {
        let fx = fixture();
        let roster = parse_org_roster(&roster_json(&fx), &fx.founder).unwrap();
        assert!(handoff_creates_work(
            Some(&roster),
            &fx.hermes,
            &fx.cody,
            &fx.founder
        ));
        assert!(handoff_creates_work(
            Some(&roster),
            &fx.founder,
            &fx.cody,
            &fx.founder
        ));
        assert!(!handoff_creates_work(
            Some(&roster),
            &fx.peer,
            &fx.cody,
            &fx.founder
        ));
        assert!(handoff_creates_work(
            None,
            &fx.founder,
            &fx.cody,
            &fx.founder
        ));
        assert!(!handoff_creates_work(None, &fx.peer, &fx.cody, &fx.founder));
    }

    #[test]
    fn peer_handoff_does_not_create_work() {
        let fx = fixture();
        let roster = parse_org_roster(&roster_json(&fx), &fx.founder).unwrap();
        assert!(!author_in_manager_chain(&roster, &fx.peer, &fx.cody).unwrap());
    }

    #[test]
    fn unknown_agents_rejected() {
        let fx = fixture();
        let roster = parse_org_roster(&roster_json(&fx), &fx.founder).unwrap();
        let mut known = HashSet::new();
        known.insert(fx.hermes.clone());
        assert!(matches!(
            reject_unknown_agents(&roster, &known),
            Err(OrgRosterError::UnknownAgent(_))
        ));
        known.insert(fx.cody.clone());
        assert!(reject_unknown_agents(&roster, &known).is_ok());
    }

    #[test]
    fn sub_kickoff_requires_parent_link() {
        let fx = fixture();
        assert!(require_parent_link_for_sub_kickoff(&fx.hermes, &fx.founder, false).is_err());
        assert!(require_parent_link_for_sub_kickoff(&fx.hermes, &fx.founder, true).is_ok());
        assert!(require_parent_link_for_sub_kickoff(&fx.founder, &fx.founder, false).is_ok());
    }

    #[test]
    fn parent_e_tag_detection() {
        assert!(!has_parent_e_tag(&[]));
        assert!(has_parent_e_tag(&[vec![
            "e".into(),
            "ab".repeat(32),
            String::new(),
            "root".into()
        ]]));
    }

    #[test]
    fn d_tag_must_be_org() {
        let fx = fixture();
        let json = r#"{"nodes":{}}"#;
        assert!(matches!(
            parse_org_roster_event(json, &fx.founder, &[vec!["d".into(), "other".into()]]),
            Err(OrgRosterError::InvalidDTag)
        ));
        assert!(parse_org_roster_event(
            json,
            &fx.founder,
            &[vec!["d".into(), ORG_ROSTER_D_TAG.into()]]
        )
        .is_ok());
    }

    #[test]
    fn handoff_tag_parses_and_ignores_extra() {
        let fx = fixture();
        let tag = vec![
            CREW_HANDOFF_TAG.to_string(),
            fx.hermes.clone(),
            goal_digest("ship it"),
            "extra".into(),
        ];
        let parsed = parse_handoff_tag(&[tag]).unwrap();
        assert_eq!(parsed.executor, fx.hermes);
        assert_eq!(parsed.goal_digest, goal_digest("ship it"));
    }

    #[test]
    fn compose_org_section_includes_org_check() {
        let fx = fixture();
        let roster = parse_org_roster(&roster_json(&fx), &fx.founder).unwrap();
        let section = compose_org_section(&roster, &fx.hermes).unwrap();
        assert!(section.contains("ORG-CHECK:"));
        assert!(section.contains("eng"));
        assert!(section.contains(&fx.cody));
    }

    #[test]
    fn serialize_round_trip() {
        let fx = fixture();
        let roster = parse_org_roster(&roster_json(&fx), &fx.founder).unwrap();
        let json = serialize_org_roster(&roster).unwrap();
        let again = parse_org_roster(&json, &fx.founder).unwrap();
        assert_eq!(roster.nodes, again.nodes);
    }

    #[test]
    fn stewardship_reads_maintainer_and_steward_tags() {
        let fx = fixture();
        let tags = vec![
            vec!["maintainers".into(), fx.hermes.clone()],
            vec!["steward".into(), fx.cody.clone()],
        ];
        let keys = stewardship_pubkeys(&tags);
        assert_eq!(keys, vec![fx.hermes.clone(), fx.cody.clone()]);
        let portfolio = portfolio_for_agent(&[("nuncio".into(), tags)], &fx.hermes).unwrap();
        assert_eq!(portfolio, vec!["nuncio".to_string()]);
    }

    #[test]
    fn is_officer_when_has_reports() {
        let fx = fixture();
        let roster = parse_org_roster(&roster_json(&fx), &fx.founder).unwrap();
        assert!(is_officer(&roster, &fx.hermes));
        assert!(!is_officer(&roster, &fx.cody));
    }

    #[test]
    fn empty_roster_is_valid() {
        let fx = fixture();
        let roster = parse_org_roster(r#"{"nodes":{}}"#, &fx.founder).unwrap();
        assert!(roster.nodes.is_empty());
    }
}
