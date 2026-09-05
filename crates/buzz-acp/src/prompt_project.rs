//! Parse a channel's authoritative NIP-MP project home for ACP `[Context]`.

use std::collections::HashMap;

use serde_json::Value;

/// Project identity attached to a home channel in agent prompts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptProjectInfo {
    pub name: String,
    pub slug: String,
    pub owner: String,
    pub coordinate: String,
    pub default_repo_owner: Option<String>,
    pub default_repo_id: Option<String>,
}

/// Resolve one listed project whose member repository authoritatively binds the channel.
///
/// A project's own `buzz-channel` is presentation metadata and cannot establish
/// authority. A candidate is accepted only when one of its `a` members resolves
/// to a live `kind:30617` whose first `buzz-channel` is `channel_id` and whose
/// owner (or `maintainers`) authorizes the project signer. Ambiguity fails closed.
pub fn pick_authoritative_project_home(
    project_events: &[Value],
    repo_events: &[Value],
    channel_id: &str,
) -> Option<PromptProjectInfo> {
    let repos = authoritative_channel_repos(repo_events, channel_id);
    let mut matches = project_events.iter().filter_map(|event| {
        if event_is_unlisted(event) || !event_has_tag_value(event, "buzz-channel", channel_id) {
            return None;
        }
        let mut project = parse_prompt_project(event)?;
        let signer = project.owner.as_str();
        let authoritative_member = event
            .get("tags")?
            .as_array()?
            .iter()
            .filter_map(|tag| tag.as_array())
            .filter(|tag| tag.first().and_then(Value::as_str) == Some("a"))
            .filter_map(|tag| tag.get(1).and_then(Value::as_str))
            .filter_map(parse_repo_coord)
            .find(|(owner, id)| {
                repos
                    .get(&(owner.clone(), id.clone()))
                    .is_some_and(|maintainers| {
                        owner.eq_ignore_ascii_case(signer)
                            || maintainers.iter().any(|m| m.eq_ignore_ascii_case(signer))
                    })
            })?;
        project.default_repo_owner = Some(authoritative_member.0);
        project.default_repo_id = Some(authoritative_member.1);
        Some(project)
    });
    let home = matches.next()?;
    matches.next().is_none().then_some(home)
}

fn authoritative_channel_repos(
    events: &[Value],
    channel_id: &str,
) -> HashMap<(String, String), Vec<String>> {
    events
        .iter()
        .filter_map(|event| {
            if event.get("kind").and_then(Value::as_u64) != Some(30617)
                || event_is_unlisted(event)
                || first_tag_value(event, "buzz-channel") != Some(channel_id)
            {
                return None;
            }
            let owner = event.get("pubkey")?.as_str()?.trim().to_ascii_lowercase();
            if owner.len() != 64 {
                return None;
            }
            let id = first_tag_value(event, "d")?.trim();
            if id.is_empty() {
                return None;
            }
            let maintainers = multi_tag_values(event, "maintainers")
                .map(str::to_ascii_lowercase)
                .collect();
            Some(((owner, id.to_string()), maintainers))
        })
        .collect()
}

fn first_tag_value<'a>(event: &'a Value, name: &'static str) -> Option<&'a str> {
    tag_values(event, name).next()
}

fn tag_values<'a>(event: &'a Value, name: &'static str) -> impl Iterator<Item = &'a str> {
    event
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .filter(move |tag| tag.first().and_then(Value::as_str) == Some(name))
        .filter_map(|tag| tag.get(1).and_then(Value::as_str))
}

fn multi_tag_values<'a>(event: &'a Value, name: &'static str) -> impl Iterator<Item = &'a str> {
    event
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .filter(move |tag| tag.first().and_then(Value::as_str) == Some(name))
        .flat_map(|tag| tag.iter().skip(1).filter_map(Value::as_str))
}

fn event_has_tag_value(event: &Value, name: &'static str, value: &str) -> bool {
    tag_values(event, name).any(|candidate| candidate == value)
}

fn event_is_unlisted(event: &Value) -> bool {
    event_has_tag_value(event, "buzz-visibility", "unlisted")
}

fn parse_prompt_project(event: &Value) -> Option<PromptProjectInfo> {
    if event.get("kind").and_then(Value::as_u64) != Some(30621) {
        return None;
    }
    let owner = event.get("pubkey")?.as_str()?.trim().to_ascii_lowercase();
    if owner.len() != 64 {
        return None;
    }
    let slug = first_tag_value(event, "d")?.trim().to_string();
    if slug.is_empty() {
        return None;
    }
    let name = first_tag_value(event, "name")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&slug)
        .to_string();
    Some(PromptProjectInfo {
        name,
        coordinate: format!("30621:{owner}:{slug}"),
        slug,
        owner,
        default_repo_owner: None,
        default_repo_id: None,
    })
}

fn parse_repo_coord(value: &str) -> Option<(String, String)> {
    let mut parts = value.splitn(3, ':');
    let kind = parts.next()?;
    let owner = parts.next()?.trim().to_ascii_lowercase();
    let id = parts.next()?.trim();
    if kind != "30617" || owner.len() != 64 || id.is_empty() {
        return None;
    }
    Some((owner, id.to_string()))
}

#[cfg(test)]
#[path = "prompt-project-tests.rs"]
mod tests;
