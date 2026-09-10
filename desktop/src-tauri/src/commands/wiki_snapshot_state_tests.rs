use super::*;

#[test]
fn owner_and_relay_state_selection_bind_to_their_signing_identity() {
    let owner_keys = keys(1);
    let relay_keys = keys(2);
    let owner = owner_keys.public_key().to_hex();
    let relay = relay_keys.public_key().to_hex();
    let coordinate = coordinate(&owner);
    let owner_event = state(
        &owner_keys,
        vec![
            vec!["d".into(), "repo".into()],
            vec!["a".into(), coordinate.clone()],
            vec!["HEAD".into(), "ref: refs/heads/main".into()],
            vec!["refs/heads/main".into(), "a".repeat(40)],
        ],
        10,
    );
    let relay_event = state(
        &relay_keys,
        vec![
            vec!["d".into(), "repo".into()],
            vec!["a".into(), coordinate.clone()],
            vec!["HEAD".into(), "ref: refs/heads/main".into()],
            vec!["refs/heads/main".into(), "b".repeat(40)],
        ],
        11,
    );

    assert!(
        select_owner_state(&[owner_event.clone()], &owner, "repo", &coordinate)
            .expect("owner state")
            .is_some()
    );
    assert!(
        select_owner_state(&[relay_event.clone()], &owner, "repo", &coordinate)
            .expect("foreign owner state")
            .is_none()
    );
    assert!(
        select_relay_state(&[relay_event], &relay, "repo", &coordinate)
            .expect("relay state")
            .is_some()
    );
    assert!(
        select_relay_state(&[owner_event], &relay, "repo", &coordinate)
            .expect("foreign relay state")
            .is_none()
    );
}

#[test]
fn owner_state_may_be_d_only_but_relay_state_requires_the_exact_association() {
    let owner_keys = keys(6);
    let relay_keys = keys(7);
    let owner = owner_keys.public_key().to_hex();
    let relay = relay_keys.public_key().to_hex();
    let coordinate = coordinate(&owner);
    let owner_d_only = state(
        &owner_keys,
        vec![
            vec!["d".into(), "repo".into()],
            vec!["HEAD".into(), "ref: refs/heads/main".into()],
            vec!["refs/heads/main".into(), "a".repeat(40)],
        ],
        10,
    );
    assert!(
        select_owner_state(&[owner_d_only], &owner, "repo", &coordinate)
            .expect("d-only owner state")
            .is_some()
    );

    let relay_d_only = state(
        &relay_keys,
        vec![
            vec!["d".into(), "repo".into()],
            vec!["HEAD".into(), "ref: refs/heads/main".into()],
            vec!["refs/heads/main".into(), "b".repeat(40)],
        ],
        11,
    );
    assert!(select_relay_state(&[relay_d_only], &relay, "repo", &coordinate).is_err());
}

#[test]
fn conflicting_repository_associations_are_rejected() {
    let owner_keys = keys(8);
    let owner = owner_keys.public_key().to_hex();
    let coordinate = coordinate(&owner);
    let conflicting = state(
        &owner_keys,
        vec![
            vec!["d".into(), "repo".into()],
            vec!["a".into(), coordinate.clone()],
            vec!["a".into(), "30617:{}:other".replace("{}", &owner)],
            vec!["HEAD".into(), "ref: refs/heads/main".into()],
            vec!["refs/heads/main".into(), "c".repeat(40)],
        ],
        10,
    );
    assert!(select_owner_state(&[conflicting], &owner, "repo", &coordinate).is_err());
}

#[test]
fn newest_owner_or_relay_state_wins_by_created_at_then_id() {
    let owner_keys = keys(9);
    let relay_keys = keys(10);
    let owner = owner_keys.public_key().to_hex();
    let relay = relay_keys.public_key().to_hex();
    let coordinate = coordinate(&owner);
    let owner_event = state(
        &owner_keys,
        vec![
            vec!["d".into(), "repo".into()],
            vec!["HEAD".into(), "ref: refs/heads/main".into()],
            vec!["refs/heads/main".into(), "a".repeat(40)],
        ],
        10,
    );
    let relay_event = state(
        &relay_keys,
        vec![
            vec!["d".into(), "repo".into()],
            vec!["a".into(), coordinate],
            vec!["HEAD".into(), "ref: refs/heads/main".into()],
            vec!["refs/heads/main".into(), "b".repeat(40)],
        ],
        11,
    );
    assert_eq!(
        newest_state(Some(owner_event), Some(relay_event.clone()))
            .expect("newest state")
            .id,
        relay_event.id
    );
}

#[tokio::test]
async fn trusted_repo_state_falls_back_to_owner_state_when_relay_self_is_unavailable() {
    let owner_keys = keys(11);
    let owner = owner_keys.public_key().to_hex();
    let owner_state = state(
        &owner_keys,
        vec![
            vec!["d".into(), "repo".into()],
            vec!["HEAD".into(), "ref: refs/heads/main".into()],
            vec!["refs/heads/main".into(), "a".repeat(40)],
        ],
        10,
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let app = mock_app_with_origin(listener.local_addr().expect("address"));
    let captured = capture(app.handle().clone()).await.expect("scope");
    let transport = OwnerOperationTransport::captured(
        &app.state::<AppState>(),
        captured.token.scope.community.clone(),
        captured.keys.clone(),
        None,
    )
    .expect("transport");
    let server = tokio::spawn(owner_state_relay(listener, owner_state.clone()));
    let result = trusted_repo_state(
        app.handle().clone(),
        &captured.token,
        &transport,
        &owner,
        "repo",
    )
    .await
    .expect("owner state fallback");
    assert_eq!(
        result.expect("owner state").id.to_hex(),
        owner_state.id.to_hex()
    );
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn reader_returns_missing_when_the_replaceable_head_is_absent() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let app = mock_app_with_origin(listener.local_addr().expect("address"));
    let captured = capture(app.handle().clone()).await.expect("scope");
    let server = tokio::spawn(empty_snapshot_relay(listener));
    let repository = coordinate(&captured.token.scope.owner);
    let result = read_wiki_snapshot(app.handle().clone(), captured.token, repository)
        .await
        .expect("missing head should be a read result")
        .value;
    assert!(matches!(result.state, WikiSnapshotReadState::Missing));
    assert!(result.head.is_none());
    assert!(result.pages.is_empty());
    server.abort();
    let _ = server.await;
}
