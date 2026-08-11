use buzz_core::crew_role::{resolve_capabilities, CAPABILITY_DEV_MCP};

#[test]
fn recognized_capability_grants_dev_mcp() {
    let owner = "11".repeat(32);
    let agent = "22".repeat(32);
    let canvas = format!(
        "```crew\nassignments:\n  \"{agent}\": backend\ndefinitions:\n  backend: Build services.\ncapabilities:\n  backend: [\"{CAPABILITY_DEV_MCP}\"]\n```"
    );
    assert_eq!(
        resolve_capabilities(&canvas, &owner, &owner, &agent).expect("valid capabilities"),
        Some(vec![CAPABILITY_DEV_MCP.to_string()])
    );
}

#[test]
fn missing_or_unrecognized_capability_fails_closed() {
    let owner = "11".repeat(32);
    let agent = "22".repeat(32);
    let canvas = format!(
        "```crew\nassignments:\n  \"{agent}\": backend\ndefinitions:\n  backend: Build services.\ncapabilities:\n  backend: [\"unknown-key\"]\n```"
    );
    assert_eq!(
        resolve_capabilities(&canvas, &owner, &owner, &agent)
            .expect("invalid capability warns and fails closed"),
        Some(Vec::new())
    );
}

#[test]
fn two_channels_can_resolve_different_capabilities_without_respawn() {
    let owner = "11".repeat(32);
    let agent = "22".repeat(32);
    let granted = format!(
        "```crew\nassignments:\n  \"{agent}\": backend\ndefinitions:\n  backend: Build services.\ncapabilities:\n  backend: [\"{CAPABILITY_DEV_MCP}\"]\n```"
    );
    let denied = format!(
        "```crew\nassignments:\n  \"{agent}\": backend\ndefinitions:\n  backend: Build services.\ncapabilities:\n  backend: []\n```"
    );
    assert_eq!(
        resolve_capabilities(&granted, &owner, &owner, &agent).expect("granted channel"),
        Some(vec![CAPABILITY_DEV_MCP.to_string()])
    );
    assert_eq!(
        resolve_capabilities(&denied, &owner, &owner, &agent).expect("denied channel"),
        Some(Vec::new())
    );
}

#[test]
fn non_founder_and_no_crew_keep_existing_behavior() {
    let owner = "11".repeat(32);
    let agent = "22".repeat(32);
    assert_eq!(
        resolve_capabilities(
            "```crew\ncapabilities:\n  backend: [\"buzz-dev-mcp\"]\n```",
            &"33".repeat(32),
            &owner,
            &agent,
        )
        .expect("authority check"),
        None
    );
    assert_eq!(
        resolve_capabilities("Founder prose.", &owner, &owner, &agent).expect("no crew"),
        None
    );
}
