//! Founder-signed org roster ingest (Crew `KIND_ORG_ROSTER`).
//!
//! Validation runs **before** storage. A broken tree must never exist on the
//! relay — unlike git-repo name reservation, which only logs after insert.

use std::collections::HashSet;

use buzz_core::org_roster::{
    parse_org_roster_event, reject_unknown_agents, OrgRoster, OrgRosterError,
};
use buzz_core::tenant::TenantContext;
use nostr::Event;

use crate::state::AppState;

use super::ingest::IngestError;

/// Convert event tags to the string-vec shape the roster parser expects.
pub fn event_tag_vecs(event: &Event) -> Vec<Vec<String>> {
    event
        .tags
        .iter()
        .map(|tag| tag.as_slice().to_vec())
        .collect()
}

/// Sync validation: founder author, `d=org`, tree, known members.
pub fn validate_org_roster_event(
    event: &Event,
    founder_pubkey: &str,
    known_members: &HashSet<String>,
) -> Result<OrgRoster, OrgRosterError> {
    let author = event.pubkey.to_hex();
    if !author.eq_ignore_ascii_case(founder_pubkey) {
        return Err(OrgRosterError::NonFounder);
    }
    let tags = event_tag_vecs(event);
    let roster = parse_org_roster_event(&event.content, &author, &tags)?;
    reject_unknown_agents(&roster, known_members)?;
    Ok(roster)
}

/// Pre-storage ingest gate for kind 30680.
pub async fn validate_org_roster_ingest(
    tenant: &TenantContext,
    event: &Event,
    state: &AppState,
) -> Result<(), IngestError> {
    let author = event.pubkey.to_hex();
    let member = state
        .db
        .get_relay_member(tenant.community(), &author)
        .await
        .map_err(|e| {
            IngestError::Internal(format!("error: db error checking org roster author: {e}"))
        })?;
    let Some(member) = member else {
        return Err(IngestError::Rejected(format!(
            "invalid: {}",
            OrgRosterError::NonFounder
        )));
    };
    if member.role != "owner" {
        return Err(IngestError::Rejected(format!(
            "invalid: {}",
            OrgRosterError::NonFounder
        )));
    }

    let members = state
        .db
        .list_relay_members(tenant.community())
        .await
        .map_err(|e| {
            IngestError::Internal(format!("error: db error listing org roster members: {e}"))
        })?;
    let known: HashSet<String> = members
        .into_iter()
        .map(|row| row.pubkey.to_ascii_lowercase())
        .collect();

    validate_org_roster_event(event, &author, &known)
        .map_err(|err| IngestError::Rejected(format!("invalid: {err}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use buzz_core::kind::KIND_ORG_ROSTER;
    use buzz_core::org_roster::{OrgRosterError, ORG_ROSTER_D_TAG};
    use nostr::{EventBuilder, Keys, Kind, Tag};

    fn signed_roster(keys: &Keys, content: &str, d: &str) -> Event {
        EventBuilder::new(Kind::Custom(KIND_ORG_ROSTER as u16), content)
            .tags([Tag::parse(["d", d]).expect("d tag")])
            .sign_with_keys(keys)
            .expect("sign")
    }

    fn hex(keys: &Keys) -> String {
        keys.public_key().to_hex()
    }

    #[test]
    fn founder_empty_roster_accepted() {
        let founder = Keys::generate();
        let event = signed_roster(&founder, r#"{"nodes":{}}"#, ORG_ROSTER_D_TAG);
        let known = HashSet::new();
        assert!(validate_org_roster_event(&event, &hex(&founder), &known).is_ok());
    }

    #[test]
    fn non_founder_rejected() {
        let founder = Keys::generate();
        let other = Keys::generate();
        let event = signed_roster(&other, r#"{"nodes":{}}"#, ORG_ROSTER_D_TAG);
        let err = validate_org_roster_event(&event, &hex(&founder), &HashSet::new()).unwrap_err();
        assert_eq!(err, OrgRosterError::NonFounder);
    }

    #[test]
    fn wrong_d_tag_rejected() {
        let founder = Keys::generate();
        let event = signed_roster(&founder, r#"{"nodes":{}}"#, "not-org");
        let err = validate_org_roster_event(&event, &hex(&founder), &HashSet::new()).unwrap_err();
        assert_eq!(err, OrgRosterError::InvalidDTag);
    }

    #[test]
    fn cycle_rejected() {
        let founder = Keys::generate();
        let a = Keys::generate();
        let b = Keys::generate();
        let a_hex = hex(&a);
        let b_hex = hex(&b);
        let content = format!(
            r#"{{"nodes":{{"{a_hex}":{{"manager":"{b_hex}","domain":"eng","budget":{{"tokens_per_day":10,"open_work_cap":1}}}},"{b_hex}":{{"manager":"{a_hex}","domain":"ops","budget":{{"tokens_per_day":10,"open_work_cap":1}}}}}}}}"#
        );
        let event = signed_roster(&founder, &content, ORG_ROSTER_D_TAG);
        let mut known = HashSet::new();
        known.insert(a_hex);
        known.insert(b_hex);
        assert!(matches!(
            validate_org_roster_event(&event, &hex(&founder), &known),
            Err(OrgRosterError::Cycle(_))
        ));
    }

    #[test]
    fn orphan_rejected() {
        let founder = Keys::generate();
        let agent = Keys::generate();
        let stranger = Keys::generate();
        let content = format!(
            r#"{{"nodes":{{"{}":{{"manager":"{}","domain":"eng","budget":{{"tokens_per_day":10,"open_work_cap":1}}}}}}}}"#,
            hex(&agent),
            hex(&stranger)
        );
        let event = signed_roster(&founder, &content, ORG_ROSTER_D_TAG);
        let mut known = HashSet::new();
        known.insert(hex(&agent));
        assert!(matches!(
            validate_org_roster_event(&event, &hex(&founder), &known),
            Err(OrgRosterError::Orphan(_, _))
        ));
    }

    #[test]
    fn unknown_agent_rejected() {
        let founder = Keys::generate();
        let agent = Keys::generate();
        let content = format!(
            r#"{{"nodes":{{"{}":{{"manager":"{}","domain":"eng","budget":{{"tokens_per_day":10,"open_work_cap":1}}}}}}}}"#,
            hex(&agent),
            hex(&founder)
        );
        let event = signed_roster(&founder, &content, ORG_ROSTER_D_TAG);
        let err = validate_org_roster_event(&event, &hex(&founder), &HashSet::new()).unwrap_err();
        assert!(matches!(err, OrgRosterError::UnknownAgent(_)));
    }

    #[test]
    fn atomic_reorg_replaces_tree() {
        let founder = Keys::generate();
        let officer = Keys::generate();
        let ic = Keys::generate();
        let first = format!(
            r#"{{"nodes":{{"{}":{{"manager":"{}","domain":"eng","budget":{{"tokens_per_day":80,"open_work_cap":3}}}},"{}":{{"manager":"{}","domain":"impl","budget":{{"tokens_per_day":40,"open_work_cap":2}}}}}}}}"#,
            hex(&officer),
            hex(&founder),
            hex(&ic),
            hex(&officer)
        );
        let second = format!(
            r#"{{"nodes":{{"{}":{{"manager":"{}","domain":"eng","budget":{{"tokens_per_day":80,"open_work_cap":3}}}}}}}}"#,
            hex(&officer),
            hex(&founder)
        );
        let mut known = HashSet::new();
        known.insert(hex(&officer));
        known.insert(hex(&ic));
        let first_event = signed_roster(&founder, &first, ORG_ROSTER_D_TAG);
        let second_event = signed_roster(&founder, &second, ORG_ROSTER_D_TAG);
        let parsed_first = validate_org_roster_event(&first_event, &hex(&founder), &known).unwrap();
        assert_eq!(parsed_first.nodes.len(), 2);
        let parsed_second =
            validate_org_roster_event(&second_event, &hex(&founder), &known).unwrap();
        assert_eq!(parsed_second.nodes.len(), 1);
        assert!(!parsed_second.nodes.contains_key(&hex(&ic)));
    }
}
