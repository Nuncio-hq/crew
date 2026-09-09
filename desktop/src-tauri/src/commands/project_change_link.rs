//! Native validation for the existing-channel Project mutation.
//! Live-head and channel eligibility reads remain the dispatcher's responsibility.

use nostr::{Event, PublicKey, Tag};

const RELATED_CHANNEL: &str = "buzz-related-channel";
const MAX_RELATED_CHANNELS: usize = 64;
const PROJECT_CHANNEL: &str = "buzz-channel";
const LOCAL_LOCATION: &str = "buzz-location";

/// Build the only permitted tag delta from an authenticated, signature-checked head.
/// None means the exact channel is already linked and no write is needed.
pub(super) fn link_channel_tags(
    head: &Event,
    owner: PublicKey,
    channel_id: &str,
) -> Result<Option<Vec<Tag>>, String> {
    if head.kind.as_u16() as u32 != buzz_core_pkg::kind::KIND_PROJECT
        || head.pubkey != owner
        || head.verify().is_err()
    {
        return Err("Project head does not match the native signing owner.".into());
    }
    buzz_sdk_pkg::builders::validate_project_envelope(head.tags.as_slice(), &head.content)
        .map_err(|error| error.to_string())?;
    let channel = uuid::Uuid::parse_str(channel_id)
        .map_err(|_| "Project channel must be a canonical UUID.".to_string())?;
    if channel.to_string() != channel_id {
        return Err("Project channel must be a canonical UUID.".into());
    }
    if head.tags.iter().any(|tag| {
        let values = tag.as_slice();
        values.first().is_some_and(|name| name == "buzz-channel")
            && values.get(1).is_some_and(|value| value == channel_id)
    }) {
        return Err("That channel is already this project's home.".into());
    }
    let related: Vec<_> = head
        .tags
        .iter()
        .filter(|tag| {
            tag.as_slice()
                .first()
                .is_some_and(|name| name == RELATED_CHANNEL)
        })
        .collect();
    if related.iter().any(|tag| tag.as_slice().len() != 2) {
        return Err("Project contains invalid related channel metadata.".into());
    }
    if related.iter().any(|tag| tag.as_slice()[1] == channel_id) {
        return Ok(None);
    }
    if related.len() >= MAX_RELATED_CHANNELS {
        return Err("Project already has the maximum number of related channels.".into());
    }
    let mut tags: Vec<Tag> = head
        .tags
        .iter()
        .filter(|tag| {
            !tag.as_slice()
                .first()
                .is_some_and(|name| name == "expected-revision")
        })
        .cloned()
        .collect();
    tags.push(Tag::parse([RELATED_CHANNEL, channel_id]).map_err(|error| error.to_string())?);
    tags.push(
        Tag::parse(["expected-revision", &head.id.to_hex()]).map_err(|error| error.to_string())?,
    );
    buzz_sdk_pkg::builders::validate_project_envelope(&tags, &head.content)
        .map_err(|error| error.to_string())?;
    Ok(Some(tags))
}

/// Revalidate a journal's signed event; generic recovery JSON grants no write authority.
/// The dispatcher must additionally confirm live revision, channel eligibility and scope.
pub(super) fn validate_signed_link(
    head: &Event,
    signed: &Event,
    owner: PublicKey,
    channel_id: &str,
) -> Result<(), String> {
    let tags = link_channel_tags(head, owner, channel_id)?
        .ok_or_else(|| "Channel is already linked; no publication is authorized.".to_string())?;
    if signed.pubkey != owner
        || signed.kind != head.kind
        || signed.content != head.content
        || signed.tags.as_slice() != tags.as_slice()
        || signed.created_at <= head.created_at
        || signed.verify().is_err()
    {
        return Err(
            "Stored Project publication does not match its exact channel-link intent.".into(),
        );
    }
    Ok(())
}

/// Append one exact repository member, retaining every existing member hint and tag.
/// Repository visibility and live-head authority are checked by the native dispatcher.
pub(super) fn attach_repository_tags(
    head: &Event,
    owner: PublicKey,
    repository_coordinate: &str,
) -> Result<Option<Vec<Tag>>, String> {
    if head.kind.as_u16() as u32 != buzz_core_pkg::kind::KIND_PROJECT
        || head.pubkey != owner
        || head.verify().is_err()
    {
        return Err("Project head does not match the native signing owner.".into());
    }
    buzz_sdk_pkg::builders::validate_project_envelope(head.tags.as_slice(), &head.content)
        .map_err(|error| error.to_string())?;
    buzz_sdk_pkg::builders::ProjectMemberCoord::parse_full(repository_coordinate)
        .map_err(|error| error.to_string())?;
    if head.tags.iter().any(|tag| {
        let values = tag.as_slice();
        values.first().is_some_and(|name| name == "a")
            && values
                .get(1)
                .is_some_and(|value| value == repository_coordinate)
    }) {
        return Ok(None);
    }
    let mut tags: Vec<Tag> = head
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().map(String::as_str) != Some("expected-revision"))
        .cloned()
        .collect();
    tags.push(Tag::parse(["a", repository_coordinate]).map_err(|error| error.to_string())?);
    tags.push(
        Tag::parse(["expected-revision", &head.id.to_hex()]).map_err(|error| error.to_string())?,
    );
    buzz_sdk_pkg::builders::validate_project_envelope(&tags, &head.content)
        .map_err(|error| error.to_string())?;
    Ok(Some(tags))
}

/// A recovered attachment may change only its exact one-member delta.
pub(super) fn validate_signed_attachment(
    head: &Event,
    signed: &Event,
    owner: PublicKey,
    repository_coordinate: &str,
) -> Result<(), String> {
    let tags = attach_repository_tags(head, owner, repository_coordinate)?
        .ok_or("Repository is already attached; no publication is authorized.")?;
    if signed.pubkey != owner
        || signed.kind != head.kind
        || signed.content != head.content
        || signed.tags.as_slice() != tags.as_slice()
        || signed.created_at <= head.created_at
        || signed.verify().is_err()
    {
        return Err(
            "Stored Project publication does not match its exact repository attachment.".into(),
        );
    }
    Ok(())
}

/// Remove only the local Buzz location from a repository announcement.
pub(super) fn unlink_repository_tags(
    head: &Event,
    owner: PublicKey,
    repository_coordinate: &str,
) -> Result<Option<Vec<Tag>>, String> {
    if head.kind.as_u16() as u32 != buzz_core_pkg::kind::KIND_GIT_REPO_ANNOUNCEMENT
        || head.pubkey != owner
        || head.verify().is_err()
    {
        return Err("Repository head does not match the native signing owner.".into());
    }
    let mut parts = repository_coordinate.splitn(3, ':');
    let kind = parts.next();
    let owner_hex = parts.next();
    let identifier = parts.next().filter(|value| !value.is_empty());
    let expected_owner_hex = owner.to_hex();
    if kind != Some("30617")
        || owner_hex != Some(expected_owner_hex.as_str())
        || identifier.is_none()
    {
        return Err("Repository coordinate does not match the native owner.".into());
    }
    let identifier = identifier.unwrap_or_default();
    let d_tags: Vec<_> = head
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "d"))
        .collect();
    if d_tags.len() != 1
        || !d_tags[0]
            .as_slice()
            .get(1)
            .is_some_and(|value| value == identifier)
    {
        return Err("Repository head does not match the selected coordinate.".into());
    }
    if !head.tags.iter().any(|tag| {
        let values = tag.as_slice();
        values.first().is_some_and(|name| name == "buzz-location")
            && values.get(1).is_some_and(|value| value == "local")
    }) {
        return Ok(None);
    }
    let mut tags: Vec<Tag> = head
        .tags
        .iter()
        .filter(|tag| {
            let values = tag.as_slice();
            !values.first().is_some_and(|name| {
                name == "expected-revision"
                    || (name == "buzz-location"
                        && values.get(1).is_some_and(|value| value == "local"))
            })
        })
        .cloned()
        .collect();
    tags.push(
        Tag::parse(["expected-revision", &head.id.to_hex()]).map_err(|error| error.to_string())?,
    );
    Ok(Some(tags))
}

pub(super) fn validate_signed_unlink(
    head: &Event,
    signed: &Event,
    owner: PublicKey,
    repository_coordinate: &str,
) -> Result<(), String> {
    let tags = unlink_repository_tags(head, owner, repository_coordinate)?
        .ok_or("Repository has no local workspace metadata to remove.")?;
    if signed.pubkey != owner
        || signed.kind != head.kind
        || signed.content != head.content
        || signed.tags.as_slice() != tags.as_slice()
        || signed.created_at <= head.created_at
        || signed.verify().is_err()
    {
        return Err("Stored repository unlink does not match its exact intent.".into());
    }
    Ok(())
}

/// Add or replace the exact local workspace metadata on one repository head.
///
/// This is the durable counterpart of the renderer's legacy helper. It keeps
/// every existing tag (including unknown future metadata) in its original
/// order, removes only the transient `auth` and previous local location, and
/// appends one conditional revision tag. The selected path is opaque metadata;
/// validate its envelope here without normalizing it.
pub(super) fn link_repository_tags(
    head: &Event,
    owner: PublicKey,
    repository_coordinate: &str,
    channel_id: &str,
    local_path: &str,
) -> Result<Option<Vec<Tag>>, String> {
    if head.kind.as_u16() as u32 != buzz_core_pkg::kind::KIND_GIT_REPO_ANNOUNCEMENT
        || head.pubkey != owner
        || head.verify().is_err()
    {
        return Err("Repository head does not match the native signing owner.".into());
    }
    let mut parts = repository_coordinate.splitn(3, ':');
    let kind = parts.next();
    let owner_hex = parts.next();
    let identifier = parts.next().filter(|value| !value.is_empty());
    let expected_owner_hex = owner.to_hex();
    if kind != Some("30617")
        || owner_hex != Some(expected_owner_hex.as_str())
        || identifier.is_none()
    {
        return Err("Repository coordinate does not match the native owner.".into());
    }
    let identifier = identifier.unwrap_or_default();
    let d_tags: Vec<_> = head
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "d"))
        .collect();
    if d_tags.len() != 1
        || !d_tags[0]
            .as_slice()
            .get(1)
            .is_some_and(|value| value == identifier)
    {
        return Err("Repository head does not match the selected coordinate.".into());
    }
    let channel = uuid::Uuid::parse_str(channel_id)
        .map_err(|_| "Project channel must be a canonical UUID.".to_string())?;
    if channel.to_string() != channel_id {
        return Err("Project channel must be a canonical UUID.".into());
    }
    if local_path.is_empty()
        || local_path.len() > 4096
        || !local_path.starts_with('/')
        || local_path.as_bytes().iter().any(|byte| *byte == 0)
        || local_path
            .chars()
            .any(|character| character == '\r' || character == '\n')
    {
        return Err("Choose an absolute local folder path.".into());
    }
    let channel_tags: Vec<_> = head
        .tags
        .iter()
        .filter(|tag| {
            tag.as_slice()
                .first()
                .is_some_and(|name| name == PROJECT_CHANNEL)
        })
        .collect();
    if channel_tags.len() > 1
        || channel_tags.iter().any(|tag| {
            tag.as_slice().len() != 2
                || tag
                    .as_slice()
                    .get(1)
                    .is_none_or(|value| value != channel_id)
        })
    {
        return Err("Repository has invalid Project channel metadata.".into());
    }
    if channel_tags.is_empty() {
        return Err(
            "Repository has no canonical Project channel; create or select one before linking."
                .into(),
        );
    }
    let mut tags: Vec<Tag> = head
        .tags
        .iter()
        .filter(|tag| {
            let values = tag.as_slice();
            !values.first().is_some_and(|name| {
                name == "auth"
                    || name == "expected-revision"
                    || (name == LOCAL_LOCATION
                        && values.get(1).is_some_and(|value| value == "local"))
            })
        })
        .cloned()
        .collect();
    tags.push(
        Tag::parse([LOCAL_LOCATION, "local", local_path]).map_err(|error| error.to_string())?,
    );
    tags.push(
        Tag::parse(["expected-revision", &head.id.to_hex()]).map_err(|error| error.to_string())?,
    );
    Ok(Some(tags))
}

pub(super) fn validate_signed_workspace_link(
    head: &Event,
    signed: &Event,
    owner: PublicKey,
    repository_coordinate: &str,
    channel_id: &str,
    local_path: &str,
) -> Result<(), String> {
    let tags = link_repository_tags(head, owner, repository_coordinate, channel_id, local_path)?
        .ok_or("Repository workspace link has no publication to make.")?;
    if signed.pubkey != owner
        || signed.kind != head.kind
        || signed.content != head.content
        || signed.tags.as_slice() != tags.as_slice()
        || signed.created_at <= head.created_at
        || signed.verify().is_err()
    {
        return Err("Stored repository workspace link does not match its exact intent.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Timestamp};
    const CHANNEL: &str = "12345678-1234-4234-8234-123456789abc";
    const PATH: &str = "/Users/oscar/Projects/Nuncio Crew";

    fn signed(keys: &Keys, tags: Vec<Tag>, content: &str, time: u64) -> Event {
        buzz_sdk_pkg::builders::build_project_with_tags(content, tags)
            .unwrap()
            .custom_created_at(Timestamp::from_secs(time))
            .sign_with_keys(keys)
            .unwrap()
    }

    fn signed_repository(keys: &Keys, tags: Vec<Tag>, content: &str, time: u64) -> Event {
        EventBuilder::new(
            Kind::Custom(buzz_core_pkg::kind::KIND_GIT_REPO_ANNOUNCEMENT as u16),
            content,
        )
        .tags(tags)
        .custom_created_at(Timestamp::from_secs(time))
        .sign_with_keys(keys)
        .unwrap()
    }

    #[test]
    fn native_link_preserves_unknown_tags_and_replaces_only_precondition() {
        let keys = Keys::generate();
        let tags = vec![
            Tag::parse(["d", "project"]).unwrap(),
            Tag::parse(["future-metadata", "untouched", "extra"]).unwrap(),
            Tag::parse(["expected-revision", "absent"]).unwrap(),
        ];
        let head = signed(&keys, tags.clone(), "preserved body", 1);
        let patched = link_channel_tags(&head, keys.public_key(), CHANNEL)
            .unwrap()
            .unwrap();
        assert_eq!(patched[0], tags[0]);
        assert_eq!(patched[1], tags[1]);
        assert_eq!(patched.len(), 4);
        assert_eq!(
            patched[3].as_slice(),
            ["expected-revision", head.id.to_hex().as_str()]
        );
        let event = signed(&keys, patched.clone(), &head.content, 2);
        validate_signed_link(&head, &event, keys.public_key(), CHANNEL).unwrap();
        let forged_intent = signed(&keys, patched, "unrelated content edit", 2);
        assert!(validate_signed_link(&head, &forged_intent, keys.public_key(), CHANNEL).is_err());
        assert!(
            validate_signed_link(&head, &event, Keys::generate().public_key(), CHANNEL).is_err()
        );
    }

    #[test]
    fn native_link_rejects_retargeted_journal_and_noncanonical_channel() {
        let keys = Keys::generate();
        let head = signed(&keys, vec![Tag::parse(["d", "project"]).unwrap()], "", 1);
        assert!(link_channel_tags(&head, keys.public_key(), &CHANNEL.to_uppercase()).is_err());
        let mut tags = link_channel_tags(&head, keys.public_key(), CHANNEL)
            .unwrap()
            .unwrap();
        tags[0] = Tag::parse(["d", "other-project"]).unwrap();
        let event = signed(&keys, tags, "", 2);
        assert!(validate_signed_link(&head, &event, keys.public_key(), CHANNEL).is_err());
    }

    #[test]
    fn native_workspace_link_preserves_repository_tag_order_and_exact_path() {
        let keys = Keys::generate();
        let repository = format!("30617:{}:repo", keys.public_key().to_hex());
        let head = signed_repository(
            &keys,
            vec![
                Tag::parse(["name", "Crew"]).unwrap(),
                Tag::parse(["d", "repo"]).unwrap(),
                Tag::parse([PROJECT_CHANNEL, CHANNEL]).unwrap(),
                Tag::parse(["auth", "temporary"]).unwrap(),
                Tag::parse(["future-tag", "keep", "all"]).unwrap(),
                Tag::parse(["buzz-location", "local", "/tmp/old"]).unwrap(),
                Tag::parse(["expected-revision", "stale"]).unwrap(),
            ],
            "repo body",
            1,
        );
        let tags = link_repository_tags(&head, keys.public_key(), &repository, CHANNEL, PATH)
            .unwrap()
            .unwrap();
        assert_eq!(tags[0].as_slice(), ["name", "Crew"]);
        assert_eq!(tags[1].as_slice(), ["d", "repo"]);
        assert_eq!(tags[2].as_slice(), [PROJECT_CHANNEL, CHANNEL]);
        assert_eq!(tags[3].as_slice(), ["future-tag", "keep", "all"]);
        assert_eq!(tags[4].as_slice(), ["buzz-location", "local", PATH]);
        assert_eq!(
            tags[5].as_slice(),
            ["expected-revision", head.id.to_hex().as_str()]
        );
        let event = signed_repository(&keys, tags, &head.content, 2);
        validate_signed_workspace_link(
            &head,
            &event,
            keys.public_key(),
            &repository,
            CHANNEL,
            PATH,
        )
        .unwrap();
    }

    #[test]
    fn native_workspace_link_rejects_path_normalization_and_retargeting() {
        let keys = Keys::generate();
        let repository = format!("30617:{}:repo", keys.public_key().to_hex());
        let head = signed_repository(
            &keys,
            vec![
                Tag::parse(["d", "repo"]).unwrap(),
                Tag::parse([PROJECT_CHANNEL, CHANNEL]).unwrap(),
            ],
            "",
            1,
        );
        assert!(
            link_repository_tags(&head, keys.public_key(), &repository, CHANNEL, "tmp").is_err()
        );
        assert!(link_repository_tags(
            &head,
            keys.public_key(),
            &repository,
            CHANNEL,
            "/tmp/with\nnewline"
        )
        .is_err());
        assert!(link_repository_tags(
            &head,
            keys.public_key(),
            &format!("30617:{}:other", keys.public_key().to_hex()),
            CHANNEL,
            PATH
        )
        .is_err());
    }

    #[test]
    fn native_workspace_link_rejects_missing_project_channel() {
        let keys = Keys::generate();
        let repository = format!("30617:{}:repo", keys.public_key().to_hex());
        let head = signed_repository(
            &keys,
            vec![Tag::parse(["d", "repo"]).unwrap()],
            "repo body",
            1,
        );
        let error = link_repository_tags(&head, keys.public_key(), &repository, CHANNEL, PATH)
            .expect_err("folder links must not create a channel association");
        assert!(error.contains("no canonical Project channel"));
    }

    #[test]
    fn native_unlink_keeps_noncanonical_d_tag_position() {
        let keys = Keys::generate();
        let repository = format!("30617:{}:repo", keys.public_key().to_hex());
        let head = signed_repository(
            &keys,
            vec![
                Tag::parse(["name", "Crew"]).unwrap(),
                Tag::parse(["d", "repo"]).unwrap(),
                Tag::parse(["buzz-location", "local", PATH]).unwrap(),
                Tag::parse(["future", "tag"]).unwrap(),
            ],
            "",
            1,
        );
        let tags = unlink_repository_tags(&head, keys.public_key(), &repository)
            .unwrap()
            .unwrap();
        assert_eq!(tags[0].as_slice(), ["name", "Crew"]);
        assert_eq!(tags[1].as_slice(), ["d", "repo"]);
        assert_eq!(tags[2].as_slice(), ["future", "tag"]);
        assert_eq!(
            tags[3].as_slice(),
            ["expected-revision", head.id.to_hex().as_str()]
        );
    }
}
