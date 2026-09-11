use super::*;

static REGISTRY_TEST_MUTEX: Mutex<()> = Mutex::new(());

fn event(content: impl Into<String>, created_at: u64, channel_id: &str) -> nostr::Event {
    nostr::EventBuilder::new(nostr::Kind::Custom(9), content)
        .tags([nostr::Tag::parse(["h", channel_id]).unwrap()])
        .custom_created_at(nostr::Timestamp::from_secs(created_at))
        .sign_with_keys(&nostr::Keys::generate())
        .unwrap()
}

fn signed_event(
    keys: &nostr::Keys,
    kind: u32,
    content: impl Into<String>,
    created_at: u64,
    tags: Vec<nostr::Tag>,
) -> nostr::Event {
    nostr::EventBuilder::new(nostr::Kind::Custom(kind as u16), content)
        .tags(tags)
        .custom_created_at(nostr::Timestamp::from_secs(created_at))
        .sign_with_keys(keys)
        .unwrap()
}

#[test]
fn settings_default_off_and_bounds_are_fixed() {
    let settings = default_settings();
    assert_eq!(settings.mode, RecapMode::Off);
    assert_eq!(settings.bounds, default_bounds());
}

#[test]
fn generation_registry_fences_stale_cancellation() {
    let _guard = REGISTRY_TEST_MUTEX.lock().unwrap();
    let key = "test-channel\0test-root";
    let generation = register_generation(key, "g-1").unwrap();
    assert!(generation_is_current(key, "g-1"));
    assert!(!generation_is_current(key, "g-2"));
    unregister_generation(key, "g-2");
    assert!(generation_is_current(key, "g-1"));
    generation.store(true, Ordering::Release);
    unregister_generation(key, "g-1");
    assert!(!generation_is_current(key, "g-1"));
}

#[test]
fn early_cancellation_intent_is_consumed_after_registration() {
    let _guard = REGISTRY_TEST_MUTEX.lock().unwrap();
    let key = "https://relay\0viewer\0channel\0root";
    remember_pending_cancellation(key, "g-early");
    let cancelled = register_generation(key, "g-early").unwrap();
    assert!(take_pending_cancellation(key, "g-early"));
    cancelled.store(true, Ordering::Release);
    unregister_generation(key, "g-early");
    assert!(!generation_is_current(key, "g-early"));
}

#[test]
fn settings_change_cancels_only_the_current_owner_scope() {
    let _guard = REGISTRY_TEST_MUTEX.lock().unwrap();
    let owner_generation =
        register_generation("https://relay\0viewer\0channel\0root", "g-owner").unwrap();
    let other_generation =
        register_generation("https://relay\0other\0channel\0root", "g-other").unwrap();
    cancel_scope_generations("https://relay", "viewer");
    assert!(owner_generation.load(Ordering::Acquire));
    assert!(!other_generation.load(Ordering::Acquire));
    unregister_generation("https://relay\0viewer\0channel\0root", "g-owner");
    unregister_generation("https://relay\0other\0channel\0root", "g-other");
}

#[test]
fn cache_key_binds_source_and_settings_fingerprints() {
    let base = recap_key_digest("https://relay", "viewer", "channel", "root", "a", "b");
    assert_ne!(
        base,
        recap_key_digest("https://relay", "viewer", "channel", "root", "changed", "b")
    );
    assert_ne!(
        base,
        recap_key_digest("https://relay", "viewer", "channel", "root", "a", "changed")
    );
}

#[test]
fn unavailable_saved_selection_remains_recoverable_in_settings() {
    let settings = RecapSettings {
        mode: RecapMode::Manual,
        runtime_id: Some("claude".to_string()),
        capability_fingerprint: Some("old".to_string()),
        ..default_settings()
    };
    let runtimes = vec![RecapRuntimeOption {
        id: "claude".to_string(),
        label: "Claude".to_string(),
        kind: "cli".to_string(),
        availability: "unsupported".to_string(),
        reason: Some("runtime_not_ready".to_string()),
        capability_fingerprint: None,
        profiles: Vec::new(),
        models: Vec::new(),
    }];
    let (recovered, valid) = recoverable_settings(settings.clone(), &runtimes);
    assert!(!valid);
    assert_eq!(recovered, settings);
    assert_eq!(recovered.mode, RecapMode::Manual);
}

#[test]
fn ids_and_generation_names_are_canonical_and_bounded() {
    assert_eq!(
        canonical_channel_id("550e8400-e29b-41d4-a716-446655440000").unwrap(),
        "550e8400-e29b-41d4-a716-446655440000"
    );
    assert!(canonical_event_id(&"a".repeat(64)).is_ok());
    assert!(!valid_generation_id("../escape"));
    assert!(!valid_generation_id(
        &"x".repeat(MAX_GENERATION_ID_BYTES + 1)
    ));
}

#[test]
fn source_keeps_root_and_newest_fitting_whole_messages() {
    let channel_id = "550e8400-e29b-41d4-a716-446655440000";
    let root = event("root", 1, channel_id);
    let older = event("older", 2, channel_id);
    let newest_too_large = event(
        "x".repeat(super::super::recap_adapter::RECAP_INPUT_LIMIT),
        3,
        channel_id,
    );
    let root_id = root.id.to_hex();
    let source = build_thread_source(
        vec![root, older, newest_too_large],
        Vec::new(),
        channel_id,
        &root_id,
    )
    .unwrap();
    assert_eq!(source.event_ids.len(), 2);
    assert_eq!(source.omitted_message_count, 1);
    assert!(source.prompt.contains("root"));
    assert!(source.prompt.contains("older"));
    assert!(!source.prompt.contains(&"x".repeat(1024)));
}

#[test]
fn source_projects_authorized_edits_and_deletions_before_prompting() {
    let channel_id = "550e8400-e29b-41d4-a716-446655440000";
    let agent_keys = nostr::Keys::generate();
    let owner_keys = nostr::Keys::generate();
    let root = signed_event(
        &agent_keys,
        9,
        "original root",
        1,
        vec![nostr::Tag::parse(["h", channel_id]).unwrap()],
    );
    let root_id = root.id.to_hex();
    let reply = signed_event(
        &agent_keys,
        9,
        "reply to remove",
        2,
        vec![
            nostr::Tag::parse(["h", channel_id]).unwrap(),
            nostr::Tag::parse(["e", root_id.as_str(), "", "reply"]).unwrap(),
        ],
    );
    let reply_id = reply.id.to_hex();
    let edit = signed_event(
        &owner_keys,
        buzz_core_pkg::kind::KIND_STREAM_MESSAGE_EDIT,
        "edited root",
        3,
        vec![
            nostr::Tag::parse(["e", root_id.as_str()]).unwrap(),
            nostr::Tag::parse(["h", channel_id]).unwrap(),
        ],
    );
    let deletion = signed_event(
        &owner_keys,
        buzz_core_pkg::kind::KIND_DELETION,
        "",
        4,
        vec![nostr::Tag::parse(["e", reply_id.as_str()]).unwrap()],
    );
    let source = build_thread_source(
        vec![root.clone(), reply],
        vec![edit, deletion],
        channel_id,
        &root_id,
    )
    .unwrap();

    assert!(source.prompt.contains("edited root"));
    assert!(!source.prompt.contains("original root"));
    assert!(!source.prompt.contains("reply to remove"));
    assert_eq!(source.event_ids, vec![root_id]);
}

#[test]
fn source_manifest_changes_for_content_tags_and_scope() {
    let channel_id = "550e8400-e29b-41d4-a716-446655440000";
    let root = event("root", 1, channel_id);
    let root_id = root.id.to_hex();
    let first = build_thread_source(vec![root.clone()], Vec::new(), channel_id, &root_id).unwrap();
    let changed = event("changed", 1, channel_id);
    let changed_id = changed.id.to_hex();
    let changed_source =
        build_thread_source(vec![changed], Vec::new(), channel_id, &changed_id).unwrap();
    assert_ne!(first.manifest_hash, changed_source.manifest_hash);
    let other_scope = build_thread_source(
        vec![root],
        Vec::new(),
        "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
        &root_id,
    )
    .unwrap();
    assert_ne!(first.manifest_hash, other_scope.manifest_hash);
}
