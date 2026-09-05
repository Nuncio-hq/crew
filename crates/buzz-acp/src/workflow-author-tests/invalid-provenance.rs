#[test]
fn malformed_or_ambiguous_metadata_fails_closed() {
    let relay = Keys::generate();
    let owner = Keys::generate().public_key().to_hex();
    let agent = Keys::generate().public_key().to_hex();
    let relay_hex = relay.public_key().to_hex();
    let valid_mentions = [&["buzz:workflow-mention", agent.as_str()][..]];

    for event in [
        workflow_event(
            &relay,
            Some(&owner),
            &[],
            &valid_mentions,
            &[agent.as_str()],
        ),
        workflow_event(
            &relay,
            None,
            &[&["buzz:workflow", "true"]],
            &valid_mentions,
            &[agent.as_str()],
        ),
        workflow_event(
            &relay,
            Some(&owner),
            &[&["buzz:workflow", "true"], &["buzz:workflow", "true"]],
            &valid_mentions,
            &[agent.as_str()],
        ),
        workflow_event(
            &relay,
            Some(&owner),
            &[&["buzz:workflow", "true", "extra"]],
            &valid_mentions,
            &[agent.as_str()],
        ),
        workflow_event(
            &relay,
            Some(&owner),
            &[&["buzz:workflow", "true"]],
            &[&["buzz:workflow-mention", agent.as_str(), "extra"]],
            &[agent.as_str()],
        ),
        workflow_event(
            &relay,
            Some(&owner),
            &[&["buzz:workflow", "true"]],
            &[
                &["buzz:workflow-mention", agent.as_str()],
                &["buzz:workflow-mention", agent.as_str()],
            ],
            &[agent.as_str()],
        ),
        workflow_event(
            &relay,
            Some(&owner),
            &[&["buzz:workflow", "true"]],
            &[&["buzz:workflow-mention", "not-a-pubkey"]],
            &[agent.as_str()],
        ),
    ] {
        assert_eq!(
            effective_prompt_author(&event, Some(&relay_hex), &agent),
            relay_hex
        );
    }

    let duplicate_owner = workflow_event(
        &relay,
        Some(&owner),
        &[&["buzz:workflow", "true"]],
        &valid_mentions,
        &[agent.as_str()],
    );
    let mut tags: Vec<Tag> = duplicate_owner.tags.iter().cloned().collect();
    tags.push(Tag::parse(["buzz:workflow-owner", owner.as_str()]).expect("duplicate owner"));
    let duplicate_owner =
        EventBuilder::new(Kind::Custom(KIND_STREAM_MESSAGE as u16), "scheduled prompt")
            .tags(tags)
            .sign_with_keys(&relay)
            .expect("signed event");
    assert_eq!(
        effective_prompt_author(&duplicate_owner, Some(&relay_hex), &agent),
        relay_hex
    );
}

#[test]
fn wrong_kind_or_missing_relay_identity_fails_closed() {
    let relay = Keys::generate();
    let owner = Keys::generate().public_key().to_hex();
    let agent = Keys::generate().public_key().to_hex();
    let relay_hex = relay.public_key().to_hex();
    let wrong_kind = EventBuilder::new(Kind::TextNote, "scheduled prompt")
        .tags([
            Tag::parse(["buzz:workflow", "true"]).expect("marker"),
            Tag::parse(["buzz:workflow-owner", owner.as_str()]).expect("owner"),
            Tag::parse(["buzz:workflow-mention", agent.as_str()]).expect("workflow mention"),
        ])
        .sign_with_keys(&relay)
        .expect("signed event");
    assert_eq!(
        effective_prompt_author(&wrong_kind, Some(&relay_hex), &agent),
        relay_hex
    );

    let valid = workflow_event(
        &relay,
        Some(&owner),
        &[&["buzz:workflow", "true"]],
        &[&["buzz:workflow-mention", agent.as_str()]],
        &[agent.as_str()],
    );
    assert_eq!(effective_prompt_author(&valid, None, &agent), relay_hex);
}
