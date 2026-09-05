use super::*;
use serde_json::json;

const CHANNEL_ID: &str = "11111111-1111-4111-8111-111111111111";

fn project(owner: &str, slug: &str, repo: &str) -> Value {
    json!({"pubkey": owner, "kind": 30621, "tags": [
        ["d", slug], ["name", slug], ["buzz-channel", CHANNEL_ID], ["a", repo]
    ]})
}

fn repo(owner: &str, id: &str, channel: &str, extra: Vec<Value>) -> Value {
    let mut tags = vec![json!(["d", id]), json!(["buzz-channel", channel])];
    tags.extend(extra);
    json!({"pubkey": owner, "kind": 30617, "tags": tags})
}

#[test]
fn requires_repo_owned_channel_binding() {
    let owner = "a".repeat(64);
    let coord = format!("30617:{owner}:game");
    let home = pick_authoritative_project_home(
        &[project(&owner, "game", &coord)],
        &[repo(&owner, "game", CHANNEL_ID, vec![])],
        CHANNEL_ID,
    )
    .unwrap();
    assert_eq!(home.default_repo_id.as_deref(), Some("game"));

    assert!(
        pick_authoritative_project_home(&[project(&owner, "game", &coord)], &[], CHANNEL_ID)
            .is_none()
    );
}

#[test]
fn hostile_project_cannot_claim_foreign_repo() {
    let owner = "a".repeat(64);
    let attacker = "b".repeat(64);
    let coord = format!("30617:{owner}:game");
    assert!(pick_authoritative_project_home(
        &[project(&attacker, "spoof", &coord)],
        &[repo(&owner, "game", CHANNEL_ID, vec![])],
        CHANNEL_ID,
    )
    .is_none());
}

#[test]
fn repo_maintainer_can_authorize_project() {
    let owner = "a".repeat(64);
    let maintainer = "b".repeat(64);
    let coord = format!("30617:{owner}:game");
    let home = pick_authoritative_project_home(
        &[project(&maintainer, "suite", &coord)],
        &[repo(
            &owner,
            "game",
            CHANNEL_ID,
            vec![json!(["maintainers", "c".repeat(64), maintainer])],
        )],
        CHANNEL_ID,
    )
    .unwrap();
    assert_eq!(home.owner, maintainer);
}

#[test]
fn ambiguous_authoritative_projects_fail_closed() {
    let owner = "a".repeat(64);
    let coord = format!("30617:{owner}:game");
    assert!(pick_authoritative_project_home(
        &[
            project(&owner, "one", &coord),
            project(&owner, "two", &coord)
        ],
        &[repo(&owner, "game", CHANNEL_ID, vec![])],
        CHANNEL_ID,
    )
    .is_none());
}

#[test]
fn first_repo_channel_binding_is_authoritative() {
    let owner = "a".repeat(64);
    let coord = format!("30617:{owner}:game");
    let other = "22222222-2222-4222-8222-222222222222";
    let mut announcement = repo(&owner, "game", other, vec![]);
    announcement["tags"]
        .as_array_mut()
        .unwrap()
        .push(json!(["buzz-channel", CHANNEL_ID]));
    assert!(pick_authoritative_project_home(
        &[project(&owner, "game", &coord)],
        &[announcement],
        CHANNEL_ID
    )
    .is_none());
}
