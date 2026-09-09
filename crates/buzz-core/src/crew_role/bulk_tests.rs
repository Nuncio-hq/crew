use super::subject;
use nostr::{Keys, ToBech32};
use subject::{
    parse_canvas_assignments, update_canvas_crew_config, CrewConfigDraft, CrewRoleDefinition,
};

#[test]
fn crew_role_bulk_empty_fence_keeps_legacy_empty_state_and_cleanup_noop_is_byte_exact() {
    use subject::remove_canvas_crew_members;
    // The pre-change production parser accepts an empty fence as an empty
    // configuration. Pin that compatibility, not a new malformed-state rule.
    let empty = parse_canvas_assignments("```crew\n```").unwrap().unwrap();
    assert!(empty.assignments.is_empty());
    assert!(empty.definitions.is_empty());
    assert!(empty.contact_pubkey.is_none());
    let key = Keys::generate().public_key().to_hex();
    assert_eq!(
        remove_canvas_crew_members("plain prose", std::slice::from_ref(&key), 1).unwrap(),
        "plain prose"
    );
    let content = "before\n```crew\n# keep comment\ndefinitions: {Review: Inspect}\nassignments: {}\n```\nafter";
    assert_eq!(
        remove_canvas_crew_members(content, &[key], 1).unwrap(),
        content
    );
}

#[test]
fn crew_role_bulk_cleanup_preserves_preexisting_unresolved_entries() {
    use subject::remove_canvas_crew_members;
    let departing = Keys::generate().public_key().to_hex();
    let remaining = Keys::generate().public_key();
    let content = format!("```crew\ndefinitions: {{Review: Inspect}}\nassignments:\n  {departing}: Review\n  legacy-invalid: Review\n  {}: Review\n  {}: Review\nrouting: {{audit: Ghost}}\ncapabilities: {{Ghost: [future-capability]}}\n```", remaining.to_hex(), remaining.to_bech32().unwrap());
    let updated = remove_canvas_crew_members(&content, &[departing], 65536)
        .expect("unrelated unresolved data must survive cleanup");
    let parsed = parse_canvas_assignments(&updated).unwrap().unwrap();
    assert_eq!(parsed.assignments.len(), 3);
    assert_eq!(parsed.assignments["legacy-invalid"], "Review");
    assert_eq!(parsed.routing["audit"], "Ghost");
    assert_eq!(parsed.capabilities["ghost"], ["future-capability"]);
}

#[test]
fn crew_role_bulk_mutation_preserves_preexisting_dangling_references() {
    let content = "```crew\ndefinitions: {}\nrouting: {audit: Ghost}\ncapabilities: {Ghost: [future-capability]}\n```";
    let updated = update_canvas_crew_config(content, &CrewConfigDraft::default(), 65536)
        .expect("editing must not require deleting unresolved historical references");
    assert_eq!(
        parse_canvas_assignments(&updated).unwrap().unwrap().routing["audit"],
        "Ghost"
    );
}

#[test]
fn crew_role_bulk_mutation_rename_cannot_leave_source_definition() {
    let draft = CrewConfigDraft {
        definitions: vec![
            CrewRoleDefinition {
                label: "Review".into(),
                definition: "Inspect".into(),
            },
            CrewRoleDefinition {
                label: "Research".into(),
                definition: "Read".into(),
            },
        ],
        renames: [("Review".into(), "Research".into())].into(),
        ..Default::default()
    };
    assert!(update_canvas_crew_config(
        "```crew\ndefinitions: {Review: Inspect}\n```",
        &draft,
        65536
    )
    .is_err());
}

#[test]
fn crew_role_bulk_distinct_numeric_pubkeys_do_not_collide_as_floats() {
    let first = "1".repeat(64);
    let second = format!("{}2", "1".repeat(63));
    let canvas = format!("```crew\nassignments:\n  {first}: Review\n  {second}: Review\ndefinitions: {{Review: Inspect}}\n```");
    let parsed = parse_canvas_assignments(&canvas)
        .expect("distinct string pubkeys must not become equal floats")
        .unwrap();
    assert_eq!(parsed.assignments.len(), 2);
    let draft = CrewConfigDraft {
        definitions: vec![CrewRoleDefinition {
            label: "Review".into(),
            definition: "Inspect".into(),
        }],
        assignments: [
            (first.clone(), "Review".into()),
            (second.clone(), "Review".into()),
        ]
        .into(),
        ..Default::default()
    };
    let updated = update_canvas_crew_config(&canvas, &draft, 65536).unwrap();
    let parsed = parse_canvas_assignments(&updated).unwrap().unwrap();
    assert!(parsed.assignments.contains_key(&first));
    assert!(parsed.assignments.contains_key(&second));
}

#[test]
fn crew_role_bulk_mutation_keeps_legacy_numeric_role_references() {
    let draft = CrewConfigDraft {
        definitions: vec![CrewRoleDefinition {
            label: "123".into(),
            definition: "Read".into(),
        }],
        ..Default::default()
    };
    let original = "```crew\ndefinitions: {123: Read}\nrouting: {456: 123}\ncapabilities: {123: [buzz-dev-mcp]}\n```";
    let updated = update_canvas_crew_config(original, &draft, 65536)
        .expect("existing numeric string labels remain editable");
    let parsed = parse_canvas_assignments(&updated).unwrap().unwrap();
    assert_eq!(parsed.routing["456"], "123");
    assert_eq!(parsed.capabilities["123"], ["buzz-dev-mcp"]);
}

#[test]
fn crew_role_bulk_cleanup_retains_definition_and_remaining_holders() {
    use subject::remove_canvas_crew_members;
    let departing = Keys::generate().public_key();
    let remaining = Keys::generate().public_key().to_hex();
    let content = format!("```crew\ndefinitions: {{Review: Inspect}}\nassignments:\n  {}: Review\n  {remaining}: Review\ncontact: {}\nrouting: {{audit: Review}}\n```", departing.to_bech32().unwrap(), departing.to_hex());
    let updated = remove_canvas_crew_members(&content, &[departing.to_hex()], 65536).unwrap();
    let parsed = parse_canvas_assignments(&updated).unwrap().unwrap();
    assert_eq!(parsed.assignments.len(), 1);
    assert_eq!(parsed.assignments[&remaining], "Review");
    assert_eq!(parsed.definitions["review"], "Inspect");
    assert_eq!(parsed.contact_pubkey, None);
    assert_eq!(parsed.routing["audit"], "Review");
    assert_eq!(
        remove_canvas_crew_members("plain prose", &[departing.to_hex()], 65536).unwrap(),
        "plain prose"
    );
}

#[test]
fn crew_role_bulk_read_exposes_unassigned_definitions_and_foreign_parse_state() {
    use subject::read_canvas_crew_metadata;
    let owner = Keys::generate().public_key().to_hex();
    let foreign = Keys::generate().public_key().to_hex();
    let canvas = format!("```crew\ndefinitions: {{Code Review: Inspect}}\ncontact: {foreign}\n```");
    for (author, authority) in [(&owner, "owner"), (&foreign, "foreign")] {
        let read = read_canvas_crew_metadata(Some(&canvas), Some(author), &owner);
        assert_eq!(read.crew_authority, authority);
        assert_eq!(read.crew_parse_state, "valid");
        assert_eq!(read.definitions[0].role_label, "Code Review");
        assert_eq!(read.definitions[0].definition, "Inspect");
        assert_eq!(read.contact_pubkey, Some(foreign.clone()));
    }
    let invalid = read_canvas_crew_metadata(Some("```crew\n[broken"), Some(&owner), &owner);
    assert_eq!(invalid.crew_parse_state, "invalid");
    assert!(invalid.definitions.is_empty());
    let absent = read_canvas_crew_metadata(None, None, &owner);
    assert_eq!(absent.crew_authority, "absent");
    assert_eq!(absent.crew_parse_state, "absent");
}

#[test]
fn crew_role_bulk_mutation_preserves_prose_unknown_values_and_role_references() {
    let key = Keys::generate().public_key().to_hex();
    let prefix = "Founder prose.\r\n\r\n";
    let suffix = "\r\nClosing prose.\n```crew\ndefinitions: {later: ignored}\n```\n";
    let content = format!("{prefix}```crew\nassignments:\n  {key}: Review\ndefinitions:\n  Review: Inspect\nrouting:\n  audit: Review\ncapabilities:\n  Review: [buzz-dev-mcp, future-capability]\ntooling: {{extraUnknown: [1, true, text]}}\nfuture: {{nested: [one, two]}}\ncontact: {key}\n```{suffix}");
    let draft = CrewConfigDraft {
        definitions: vec![CrewRoleDefinition {
            label: "Research".into(),
            definition: "Read first".into(),
        }],
        renames: [("Review".into(), "Research".into())].into(),
        ..Default::default()
    };
    let updated = update_canvas_crew_config(&content, &draft, 65536).unwrap();
    let parsed = parse_canvas_assignments(&updated).unwrap().unwrap();
    assert_eq!(parsed.definitions["research"], "Read first");
    assert!(parsed.assignments.is_empty());
    assert_eq!(parsed.contact_pubkey, None);
    assert_eq!(parsed.routing["audit"], "Research");
    assert_eq!(
        parsed.capabilities["research"],
        ["buzz-dev-mcp", "future-capability"]
    );
    assert!(updated.starts_with(prefix));
    assert!(updated.ends_with(suffix));
    let yaml = |text: &str| -> serde_yaml::Value {
        serde_yaml::from_str(
            text.split_once("```crew\n")
                .unwrap()
                .1
                .split_once("```")
                .unwrap()
                .0,
        )
        .unwrap()
    };
    for field in ["tooling", "future"] {
        assert_eq!(yaml(&updated)[field], yaml(&content)[field]);
    }
}

#[test]
fn crew_role_bulk_mutation_rejects_unresolved_reference_and_oversize() {
    let content = "```crew\ndefinitions: {Review: Inspect}\nrouting: {audit: Review}\n```";
    assert!(update_canvas_crew_config(content, &CrewConfigDraft::default(), 65536).is_err());
    assert!(update_canvas_crew_config("prose", &CrewConfigDraft::default(), 2).is_err());
}

#[test]
fn crew_role_bulk_mutation_rename_updates_assignment_references() {
    let key = Keys::generate().public_key().to_hex();
    let draft = CrewConfigDraft {
        definitions: vec![CrewRoleDefinition {
            label: "Research".into(),
            definition: "Read".into(),
        }],
        assignments: [(key.clone(), "Review".into())].into(),
        renames: [("Review".into(), "Research".into())].into(),
        ..Default::default()
    };
    let updated = update_canvas_crew_config(
        "```crew\ndefinitions: {Review: Inspect}\n```",
        &draft,
        65536,
    )
    .expect("rename rewrites the retained assignment's exact role reference");
    assert_eq!(
        parse_canvas_assignments(&updated)
            .unwrap()
            .unwrap()
            .assignments[&key],
        "Research"
    );
}

#[test]
fn crew_role_bulk_rejects_case_colliding_role_reference_maps() {
    for yaml in [
        "routing: {Audit: Review, audit: Research}",
        "capabilities: {Review: [buzz-dev-mcp], review: []}",
    ] {
        assert!(parse_canvas_assignments(&format!("```crew\n{yaml}\n```")).is_err());
    }
}

#[test]
fn crew_role_bulk_mutation_rejects_two_renames_to_one_role() {
    let draft = CrewConfigDraft {
        definitions: vec![CrewRoleDefinition {
            label: "Research".into(),
            definition: "Read".into(),
        }],
        renames: [
            ("Review".into(), "Research".into()),
            ("Write".into(), "Research".into()),
        ]
        .into(),
        ..Default::default()
    };
    assert!(update_canvas_crew_config(
        "```crew\ndefinitions: {Review: Inspect, Write: Create}\n```",
        &draft,
        65536
    )
    .is_err());
}

#[test]
fn crew_role_bulk_mutation_validates_keys_and_output_limit() {
    let key = Keys::generate().public_key();
    let mut draft = CrewConfigDraft {
        definitions: vec![CrewRoleDefinition {
            label: "Review".into(),
            definition: "Inspect".into(),
        }],
        assignments: [
            (key.to_hex(), "Review".into()),
            (key.to_bech32().unwrap(), "Review".into()),
        ]
        .into(),
        ..Default::default()
    };
    assert!(
        update_canvas_crew_config("", &draft, 65536).is_err(),
        "canonical duplicate assignments rejected"
    );
    draft.assignments.clear();
    draft.contact = Some("invalid".into());
    assert!(update_canvas_crew_config("", &draft, 65536).is_err());
    draft.contact = None;
    assert!(
        update_canvas_crew_config("", &draft, 10).is_err(),
        "expanded output respects event cap"
    );
    let updated = update_canvas_crew_config("inline ```crew is prose", &draft, 65536).unwrap();
    assert!(updated.starts_with("inline ```crew is prose\n```crew\n"));
    assert!(update_canvas_crew_config("```crew\n[broken", &draft, 65536).is_err());
}

#[test]
fn crew_role_bulk_exposes_labels_and_optional_contact_without_assignment() {
    let contact = Keys::generate().public_key();
    for key in [contact.to_hex(), contact.to_bech32().unwrap()] {
        let parsed = parse_canvas_assignments(&format!(
            "```crew\ndefinitions:\n  Code Review: Inspect\ncontact: {key}\n```"
        ))
        .unwrap()
        .unwrap();
        assert_eq!(parsed.definition_labels["code review"], "Code Review");
        assert_eq!(parsed.contact_pubkey, Some(contact.to_hex()));
        assert!(parsed.assignments.is_empty());
    }
    for yaml in ["definitions: {}", "definitions: {}\ncontact: null"] {
        assert_eq!(
            parse_canvas_assignments(&format!("```crew\n{yaml}\n```"))
                .unwrap()
                .unwrap()
                .contact_pubkey,
            None
        );
    }
}

#[test]
fn crew_role_bulk_rejects_invalid_contact_metadata() {
    let result = parse_canvas_assignments("```crew\ncontact: not-a-public-key\n```");
    assert!(
        result.is_err(),
        "invalid contact must not be silently ignored"
    );
}

#[test]
fn crew_role_bulk_rejects_case_colliding_definitions() {
    let result = parse_canvas_assignments(
        "```crew\ndefinitions:\n  Review: first meaning\n  review: second meaning\n```",
    );
    assert!(
        result.is_err(),
        "case collision must not discard a role meaning"
    );
}

#[test]
fn crew_role_bulk_rejects_duplicate_yaml_keys() {
    for yaml in [
        "definitions:\n  Review: first\n  Review: second",
        "definitions: {}\ndefinitions: {}",
        "future:\n  nested: first\n  nested: second",
    ] {
        assert!(
            parse_canvas_assignments(&format!("```crew\n{yaml}\n``` ")).is_err(),
            "duplicate YAML keys must not discard semantic values: {yaml}"
        );
    }
}

#[test]
fn crew_role_bulk_never_bypasses_malformed_first_fence() {
    assert!(parse_canvas_assignments(
        "before\n```crew\ndefinitions: [broken\n```\nafter\n```crew\ndefinitions: {}\n```"
    )
    .is_err());
}

#[test]
fn crew_role_bulk_keeps_unassigned_and_shared_roles() {
    let parsed = parse_canvas_assignments(
        "```crew\nassignments:\n  first: Review\n  second: Review\ndefinitions:\n  Review: Inspect\n  Research: Read\nrouting:\n  audit: Review\ncapabilities:\n  Review: [buzz-dev-mcp]\nfuture: {keep: [one, two]}\n```",
    ).unwrap().unwrap();
    assert_eq!(parsed.assignments.len(), 2);
    assert_eq!(parsed.definitions.len(), 2);
    assert_eq!(parsed.definitions["research"], "Read");
    assert_eq!(parsed.routing["audit"], "Review");
    assert_eq!(parsed.capabilities["review"], ["buzz-dev-mcp"]);
}

#[test]
fn crew_role_bulk_preserves_existing_unicode_and_byte_limits() {
    for label in ["Nghiên cứu".to_string(), "é".repeat(64)] {
        let parsed =
            parse_canvas_assignments(&format!("```crew\ndefinitions:\n  {label}: Meaning\n```"))
                .unwrap()
                .unwrap();
        assert_eq!(parsed.definitions[&label.to_ascii_lowercase()], "Meaning");
    }
    assert!(parse_canvas_assignments(&format!(
        "```crew\ndefinitions:\n  {}: Meaning\n```",
        "é".repeat(65)
    ))
    .is_err());
}
