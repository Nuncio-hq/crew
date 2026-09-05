//! Membership notifications reconcile against background subscription authority.
use std::collections::HashSet;
use uuid::Uuid;

pub(crate) fn membership_add_needs_subscribe(
    channel: Uuid,
    snapshot: &HashSet<Uuid>,
    removed_channels: &mut HashSet<Uuid>,
) -> bool {
    // A queued unsubscribe may not yet be reflected in the background snapshot.
    // Reassert the newer add so command ordering cannot strand the channel.
    let was_removed = removed_channels.remove(&channel);
    was_removed || !snapshot.contains(&channel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removal_queued_before_add_requires_subscription_even_before_transport_applies_it() {
        let channel = Uuid::new_v4();
        let unrelated = Uuid::new_v4();
        let snapshot = HashSet::from([channel, unrelated]);
        // The main loop has consumed removal, while the background task has
        // not yet dequeued its unsubscribe command. Its snapshot is still old.
        let mut removed = HashSet::from([channel, unrelated]);
        assert!(membership_add_needs_subscribe(
            channel,
            &snapshot,
            &mut removed
        ));
        assert!(
            !removed.contains(&channel),
            "re-add clears pending session invalidation"
        );
        assert!(removed.contains(&unrelated));
    }

    #[test]
    fn existing_subscription_skips_duplicate_but_denied_snapshot_allows_rejoin() {
        let channel = Uuid::new_v4();
        let mut removed = HashSet::new();
        assert!(!membership_add_needs_subscribe(
            channel,
            &HashSet::from([channel]),
            &mut removed
        ));
        assert!(membership_add_needs_subscribe(
            channel,
            &HashSet::new(),
            &mut removed
        ));
    }
}
