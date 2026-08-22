use super::project_git_workflow::{
    normalize_event_id, project_owner_identity, validate_repo_address,
};
use crate::app_state::AppState;
use crate::relay::submit_signed_event_with_keys;
use nostr::{Event, EventBuilder, JsonUtil, Keys, Kind, Tag, Timestamp};
use serde::Deserialize;
use tauri::{AppHandle, State};

/// Repository-scoped metadata for a managed-owner issue assignment operation.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIssueAssigneeOperationInput {
    target_owner: String,
    repo_address: String,
    issue_id: String,
    assignees: Vec<String>,
    assignee_label: String,
    created_at: u64,
}

fn build_issue_assignee_operation_event(
    keys: &Keys,
    input: &ProjectIssueAssigneeOperationInput,
    label: &str,
) -> Result<String, String> {
    let owner = keys.public_key().to_hex();
    validate_repo_address(&input.repo_address, &owner)?;
    let issue_id =
        normalize_event_id(&input.issue_id).ok_or_else(|| "Invalid issue event ID.".to_string())?;
    if input.assignees.is_empty() || input.assignees.len() > 50 {
        return Err("Select between 1 and 50 assignees.".to_string());
    }
    let mut assignees = input
        .assignees
        .iter()
        .map(|assignee| {
            normalize_event_id(assignee).ok_or_else(|| "Invalid assignee pubkey.".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    assignees.sort();
    assignees.dedup();
    let assignee_label = input.assignee_label.trim();
    if assignee_label.is_empty() || assignee_label.chars().count() > 128 {
        return Err("Assignee label must be between 1 and 128 characters.".to_string());
    }
    let mut raw_tags = vec![
        vec!["e".to_string(), issue_id, String::new(), "root".to_string()],
        vec!["a".to_string(), input.repo_address.clone()],
    ];
    raw_tags.extend(
        assignees
            .into_iter()
            .map(|assignee| vec!["p".to_string(), assignee]),
    );
    raw_tags.push(vec!["t".to_string(), label.to_string()]);
    let tags = raw_tags
        .into_iter()
        .map(Tag::parse)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("build issue {label} tags: {error}"))?;
    let content = if label == "assignment" {
        format!("Assigned this issue to {assignee_label}")
    } else {
        format!("Unassigned {assignee_label} from this issue")
    };
    EventBuilder::new(Kind::TextNote, content)
        .tags(tags)
        .custom_created_at(Timestamp::from(input.created_at))
        .sign_with_keys(keys)
        .map(|event| event.as_json())
        .map_err(|error| format!("sign issue {label}: {error}"))
}

/// Signs and publishes an issue assignment as the repository owner.
#[tauri::command]
pub async fn sign_project_issue_assignment(
    input: ProjectIssueAssigneeOperationInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    sign_project_issue_assignee_operation(input, "assignment", app, state).await
}

/// Signs and publishes an issue unassignment as the repository owner.
#[tauri::command]
pub async fn sign_project_issue_unassignment(
    input: ProjectIssueAssigneeOperationInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    sign_project_issue_assignee_operation(input, "unassignment", app, state).await
}

async fn sign_project_issue_assignee_operation(
    input: ProjectIssueAssigneeOperationInput,
    label: &str,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let target_owner = input.target_owner.trim().to_ascii_lowercase();
    if normalize_event_id(&target_owner).is_none() {
        return Err("Invalid target repository owner.".to_string());
    }
    let identity = project_owner_identity(&app, &state, &target_owner)?;
    let event = Event::from_json(build_issue_assignee_operation_event(
        &identity.keys,
        &input,
        label,
    )?)
    .map_err(|error| format!("parse signed issue {label}: {error}"))?;
    submit_signed_event_with_keys(&event, &state, &identity.keys, identity.auth_tag.as_deref())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_issue_assignee_operation_event, ProjectIssueAssigneeOperationInput};
    use nostr::{Event, JsonUtil, Keys};

    #[test]
    fn issue_assignment_is_signed_by_repository_owner() {
        let keys = Keys::generate();
        let owner = keys.public_key().to_hex();
        let assignee = "b".repeat(64);
        let event = Event::from_json(
            build_issue_assignee_operation_event(
                &keys,
                &ProjectIssueAssigneeOperationInput {
                    target_owner: owner.clone(),
                    repo_address: format!("30617:{owner}:buzz"),
                    issue_id: "d".repeat(64),
                    assignees: vec![assignee.clone()],
                    assignee_label: "Bob".to_string(),
                    created_at: 123,
                },
                "assignment",
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(event.pubkey, keys.public_key());
        assert_eq!(event.created_at.as_secs(), 123);
        assert_eq!(event.content, "Assigned this issue to Bob");
        assert!(event
            .tags
            .iter()
            .any(|tag| tag.as_slice() == ["p", assignee.as_str()]));
        assert!(event
            .tags
            .iter()
            .any(|tag| tag.as_slice() == ["t", "assignment"]));
        assert!(event.verify().is_ok());
    }
}
