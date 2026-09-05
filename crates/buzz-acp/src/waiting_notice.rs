//! Non-terminal checkout waiting messages must never offer failure Retry.

use crate::{queue::FlushBatch, relay::RestClient};
use std::time::Duration;

fn builder(batch: &FlushBatch, content: &str) -> Result<nostr::EventBuilder, buzz_sdk::SdkError> {
    let thread = batch.events.last().map(|entry| {
        let tags = crate::queue::parse_thread_tags(&entry.event);
        let root = tags
            .root_event_id
            .as_deref()
            .and_then(|id| nostr::EventId::from_hex(id).ok())
            .unwrap_or(entry.event.id);
        buzz_sdk::ThreadRef {
            root_event_id: root,
            parent_event_id: entry.event.id,
        }
    });
    buzz_sdk::build_message(
        batch.routing_channel_id(),
        content,
        thread.as_ref(),
        &[],
        false,
        &[],
    )
}

pub(crate) fn spawn(rest: Option<&RestClient>, batch: &FlushBatch, content: String) {
    let Some(rest) = rest.cloned() else { return };
    let message = match builder(batch, &content) {
        Ok(message) => message,
        Err(error) => {
            tracing::warn!(%error, "could not build checkout waiting notice");
            return;
        }
    };
    tokio::spawn(async move {
        let event = match message.sign_with_keys(&rest.keys) {
            Ok(event) => event,
            Err(error) => {
                tracing::warn!(%error, "could not sign checkout waiting notice");
                return;
            }
        };
        match tokio::time::timeout(Duration::from_secs(5), rest.submit_event(&event)).await {
            Ok(Ok(_)) => {}
            result => tracing::warn!(?result, "checkout waiting notice was not acknowledged"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::BatchEvent;

    #[test]
    fn waiting_notice_is_threaded_without_failure_retry_targets() {
        let keys = nostr::Keys::generate();
        let channel = uuid::Uuid::new_v4();
        let request = nostr::EventBuilder::new(nostr::Kind::Custom(9), "work")
            .sign_with_keys(&keys)
            .unwrap();
        let request_id = request.id.to_hex();
        let batch = FlushBatch {
            channel_id: channel,
            events: vec![BatchEvent {
                event: request,
                prompt_tag: "test".into(),
                received_at: std::time::Instant::now(),
                edited_content: None,
            }],
            cancelled_events: vec![],
            cancel_reason: None,
        };
        let event = builder(&batch, "Waiting for checkout")
            .unwrap()
            .sign_with_keys(&keys)
            .unwrap();
        let tags: Vec<_> = event.tags.iter().map(|tag| tag.as_slice()).collect();
        assert!(tags
            .iter()
            .any(|tag| tag.first().is_some_and(|v| v == "h")
                && tag.get(1) == Some(&channel.to_string())));
        assert!(
            tags.iter()
                .any(|tag| tag.get(1) == Some(&request_id)
                    && tag.get(3).is_some_and(|v| v == "reply"))
        );
        assert!(!tags
            .iter()
            .any(|tag| tag.first().is_some_and(|v| v == "failure_notice")
                || tag.get(3).is_some_and(|v| v == "failed")));
    }
}
