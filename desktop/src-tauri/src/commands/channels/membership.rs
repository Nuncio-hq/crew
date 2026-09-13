//! Validate the exact relay-owned channel membership snapshot before use.

use nostr::Event;

/// Select the latest verified relay snapshot for the exact channel coordinate.
/// A malformed latest coordinate is rejected without falling back to an older row.
pub(crate) fn channel_membership_snapshot<'a>(
    events: &'a [Event],
    relay_pubkey: &str,
    channel_id: &str,
) -> Result<&'a Event, String> {
    let event = events
        .iter()
        .filter(|event| {
            event.kind.as_u16() == 39002
                && event.pubkey.to_hex().eq_ignore_ascii_case(relay_pubkey)
                && event.verify().is_ok()
                && event.tags.iter().any(|tag| {
                    let values = tag.as_slice();
                    values.first().is_some_and(|name| name == "d")
                        && values.get(1).is_some_and(|value| value == channel_id)
                })
        })
        .max_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| right.id.cmp(&left.id))
        })
        .ok_or_else(|| "channel membership snapshot is unavailable".to_string())?;
    let coordinates: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "d"))
        .collect();
    if coordinates.len() != 1
        || coordinates[0].as_slice().len() != 2
        || coordinates[0].as_slice()[1] != channel_id
    {
        return Err("channel membership snapshot has an invalid channel coordinate".into());
    }
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag, Timestamp};

    fn snapshot(keys: &Keys, tags: Vec<Vec<&str>>, time: u64) -> Event {
        EventBuilder::new(Kind::Custom(39002), "")
            .tags(tags.into_iter().map(|values| Tag::parse(values).unwrap()))
            .custom_created_at(Timestamp::from(time))
            .sign_with_keys(keys)
            .unwrap()
    }

    #[test]
    fn exact_signed_channel_is_required_and_newer_malformed_head_cannot_fall_back() {
        let relay = Keys::generate();
        let signer = relay.public_key().to_hex();
        let valid = snapshot(&relay, vec![vec!["d", "channel"]], 1);
        assert!(
            channel_membership_snapshot(std::slice::from_ref(&valid), &signer, "channel").is_ok()
        );
        assert!(
            channel_membership_snapshot(std::slice::from_ref(&valid), &signer, "other").is_err()
        );
        let foreign = snapshot(&Keys::generate(), vec![vec!["d", "channel"]], 2);
        assert!(channel_membership_snapshot(&[foreign], &signer, "channel").is_err());
        let mut tampered = valid.clone();
        tampered.content = "changed after signing".into();
        assert!(channel_membership_snapshot(&[tampered], &signer, "channel").is_err());
        for tags in [
            vec![vec!["d", "channel"], vec!["d", "channel"]],
            vec![vec!["d", "channel", "extra"]],
            vec![vec!["d", "other"], vec!["d", "channel"]],
        ] {
            let malformed = snapshot(&relay, tags, 2);
            assert!(
                channel_membership_snapshot(&[valid.clone(), malformed], &signer, "channel")
                    .is_err()
            );
        }
    }
}
