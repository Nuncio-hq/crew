use crate::crew_role::{read_canvas_crew_metadata, update_canvas_crew_config, CrewConfigDraft};
use nostr::Keys;
use serde_json::json;

const CANVAS: &str = "prose\n```crew\nassignments: {bad-key: Review, other-bad: Ghost}\ndefinitions: {Review: Inspect}\nrouting: {audit: Review}\ncapabilities: {Review: [buzz-dev-mcp]}\nfuture: {keep: true}\n```\nafter";

#[test]
fn editor_read_and_save_do_not_collapse_trim_equivalent_legacy_entries() {
    let content = "```crew\nassignments: {' bad-key ': ' Review ', bad-key: Ghost}\ndefinitions: {Review: Inspect}\n```";
    let owner = Keys::generate().public_key().to_hex();
    let metadata = serde_json::to_value(read_canvas_crew_metadata(
        Some(content),
        Some(&owner),
        &owner,
    ))
    .unwrap();
    let original = json!({" bad-key ": " Review ", "bad-key": "Ghost"});
    assert_eq!(metadata["stored_assignments"], original);
    let draft: CrewConfigDraft = serde_json::from_value(json!({
        "definitions": [{"label": "Review", "definition": "Inspect"}],
        "assignments": {}, "contact": null, "preserved_assignments": original
    }))
    .unwrap();
    let result = update_canvas_crew_config(content, &draft, 65536).unwrap();
    let after = serde_json::to_value(read_canvas_crew_metadata(
        Some(&result),
        Some(&owner),
        &owner,
    ))
    .unwrap();
    assert_eq!(after["stored_assignments"], original);
}

#[test]
fn editor_read_retains_unresolved_assignments_and_reference_metadata() {
    let owner = Keys::generate().public_key().to_hex();
    let metadata = read_canvas_crew_metadata(Some(CANVAS), Some(&owner), &owner);
    let value = serde_json::to_value(metadata).unwrap();
    assert_eq!(
        value["stored_assignments"],
        json!({"bad-key": "Review", "other-bad": "Ghost"})
    );
    assert_eq!(value["stored_routing"], json!({"audit": "Review"}));
    assert_eq!(
        value["stored_capabilities"],
        json!({"review": ["buzz-dev-mcp"]})
    );
}

#[test]
fn editor_bulk_keeps_explicitly_retained_original_entries() {
    let draft: CrewConfigDraft = serde_json::from_value(json!({
        "definitions": [{"label": "Review", "definition": "Inspect carefully"}],
        "assignments": {}, "contact": null,
        "preserved_assignments": {"bad-key": "Review", "other-bad": "Ghost"}
    }))
    .unwrap();
    let result = update_canvas_crew_config(CANVAS, &draft, 65536).unwrap();
    let block = crate::crew_role::parse_canvas_assignments(&result)
        .unwrap()
        .unwrap();
    assert_eq!(
        block.assignments.get("bad-key").map(String::as_str),
        Some("Review")
    );
    assert_eq!(
        block.assignments.get("other-bad").map(String::as_str),
        Some("Ghost")
    );
    assert_eq!(block.definitions["review"], "Inspect carefully");
}

#[test]
fn editor_bulk_cannot_invent_a_retained_legacy_entry() {
    let draft: CrewConfigDraft = serde_json::from_value(json!({
        "definitions": [{"label": "Review", "definition": "Inspect"}],
        "assignments": {}, "contact": null,
        "preserved_assignments": {"bad-key": "Different"}
    }))
    .unwrap();
    assert!(update_canvas_crew_config(CANVAS, &draft, 65536).is_err());
}

#[test]
fn editor_bulk_role_delete_requires_explicit_reference_resolution() {
    let mut value = json!({"definitions": [], "assignments": {}, "contact": null});
    let unresolved: CrewConfigDraft = serde_json::from_value(value.clone()).unwrap();
    assert!(update_canvas_crew_config(CANVAS, &unresolved, 65536).is_err());
    value["remove_routing"] = json!(["audit"]);
    value["remove_capabilities"] = json!(["Review"]);
    let resolved: CrewConfigDraft = serde_json::from_value(value).unwrap();
    let result = update_canvas_crew_config(CANVAS, &resolved, 65536).unwrap();
    let block = crate::crew_role::parse_canvas_assignments(&result)
        .unwrap()
        .unwrap();
    assert!(block.definitions.is_empty());
    assert!(block.routing.is_empty());
    assert!(block.capabilities.is_empty());
    assert!(result.starts_with("prose\n") && result.ends_with("\nafter"));
    assert!(result.contains("keep: true"));
}
