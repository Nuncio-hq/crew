//! Repo stewardship helpers (Crew). Org chart Show/Tree/Publish removed (#233).

use buzz_core::kind::{KIND_GIT_REPO_ANNOUNCEMENT, KIND_PROJECT};
use buzz_core::org_roster::{
    canonical_pubkey, portfolio_for_agent, stewardship_pubkeys, MAINTAINERS_TAG, STEWARD_TAG,
};
use nostr::Tag;

use crate::client::BuzzClient;
use crate::commands::parse_write_response;
use crate::error::CliError;
use crate::validate::validate_hex64;

async fn fetch_repo_events(
    client: &BuzzClient,
) -> Result<Vec<(String, Vec<Vec<String>>)>, CliError> {
    let filter = serde_json::json!({
        "kinds": [KIND_GIT_REPO_ANNOUNCEMENT, KIND_PROJECT],
        "limit": 200
    });
    let raw = client.query(&filter).await?;
    let events: Vec<serde_json::Value> = serde_json::from_str(&raw)
        .map_err(|error| CliError::Other(format!("portfolio query is not JSON: {error}")))?;
    Ok(events
        .into_iter()
        .filter_map(|event| {
            let id = event.get("content").and_then(|v| {
                v.as_str().and_then(|content| {
                    serde_json::from_str::<serde_json::Value>(content)
                        .ok()
                        .and_then(|body| {
                            body.get("name")
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string)
                        })
                        .or_else(|| {
                            event
                                .get("tags")
                                .and_then(serde_json::Value::as_array)
                                .and_then(|tags| {
                                    tags.iter().find_map(|tag| {
                                        let cells = tag.as_array()?;
                                        (cells.first()?.as_str()? == "d")
                                            .then(|| cells.get(1)?.as_str().map(str::to_string))
                                            .flatten()
                                    })
                                })
                        })
                })
            });
            let tags: Vec<Vec<String>> = event
                .get("tags")
                .and_then(serde_json::Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(|row| {
                            row.as_array().map(|cells| {
                                cells
                                    .iter()
                                    .filter_map(|cell| cell.as_str().map(str::to_string))
                                    .collect()
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            id.map(|name| (name, tags))
        })
        .collect())
}

pub async fn dispatch(
    cmd: crate::OrgCmd,
    client: &BuzzClient,
    format: &crate::OutputFormat,
) -> Result<(), CliError> {
    let _format = format;
    match cmd {
        crate::OrgCmd::Show | crate::OrgCmd::Tree | crate::OrgCmd::Publish { .. } => {
            Err(CliError::Usage(
                "Org chart/roster CLI is removed (#233). Company model is channel roles + call-by-name; 30680 stays on the relay for sync only.".into(),
            ))
        }
        crate::OrgCmd::Portfolio { pubkey } => {
            validate_hex64(&pubkey)?;
            let agent =
                canonical_pubkey(&pubkey).map_err(|error| CliError::Usage(error.to_string()))?;
            let events = fetch_repo_events(client).await?;
            let names = portfolio_for_agent(&events, &agent)
                .map_err(|error| CliError::Other(error.to_string()))?;
            println!(
                "{}",
                serde_json::to_string(&names).unwrap_or_else(|_| "[]".into())
            );
            Ok(())
        }
        crate::OrgCmd::Steward { repo, pubkey } => {
            validate_hex64(&pubkey)?;
            let agent =
                canonical_pubkey(&pubkey).map_err(|error| CliError::Usage(error.to_string()))?;
            let filter = serde_json::json!({
                "kinds": [KIND_GIT_REPO_ANNOUNCEMENT],
                "authors": [client.keys().public_key().to_hex()],
                "#d": [repo],
                "limit": 1,
            });
            let raw = client.query(&filter).await?;
            let mut events: Vec<nostr::Event> = serde_json::from_str(&raw).map_err(|error| {
                CliError::Other(format!("failed to parse repo announcement: {error}"))
            })?;
            events.sort_by_key(|event| std::cmp::Reverse(event.created_at));
            let existing = events.into_iter().next().ok_or_else(|| {
                CliError::NotFound(format!(
                    "no kind:30617 announcement for {repo} signed by this key"
                ))
            })?;
            let mut tags: Vec<Tag> = existing.tags.iter().cloned().collect();
            let current: Vec<Vec<String>> = existing
                .tags
                .iter()
                .map(|tag| tag.as_slice().to_vec())
                .collect();
            if !stewardship_pubkeys(&current).contains(&agent) {
                let tag = Tag::parse([STEWARD_TAG, agent.as_str()])
                    .or_else(|_| Tag::parse([MAINTAINERS_TAG, agent.as_str()]))
                    .map_err(|error| CliError::Other(format!("steward tag: {error}")))?;
                tags.push(tag);
            }
            let builder =
                buzz_sdk::build_repo_announcement_with_tags(&repo, &existing.content, tags)
                    .map_err(|error| {
                        CliError::Other(format!("build repo announcement: {error}"))
                    })?;
            let event = client.sign_event(builder)?;
            let resp = client.submit_event(event).await?;
            println!(
                "{}",
                parse_write_response(&resp, "repository announcement was dominated")?
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use buzz_core::org_roster::{direct_reports, parse_org_roster};
    use nostr::Keys;

    #[test]
    fn parser_still_lists_direct_reports_for_protocol() {
        let founder = Keys::generate().public_key().to_hex();
        let officer = Keys::generate().public_key().to_hex();
        let json = format!(
            r#"{{"nodes":{{"{officer}":{{"manager":"{founder}","domain":"eng","budget":{{"tokens_per_day":10,"open_work_cap":1}}}}}}}}"#
        );
        let roster = parse_org_roster(&json, &founder).unwrap();
        let reports = direct_reports(&roster, &founder).unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].domain, "eng");
    }

    #[test]
    fn sub_kickoff_without_parent_rejected_for_non_founder() {
        let founder = Keys::generate().public_key().to_hex();
        let officer = Keys::generate().public_key().to_hex();
        assert!(buzz_core::org_roster::require_parent_link_for_sub_kickoff(
            &officer, &founder, false
        )
        .is_err());
        assert!(buzz_core::org_roster::require_parent_link_for_sub_kickoff(
            &founder, &founder, false
        )
        .is_ok());
    }
}
