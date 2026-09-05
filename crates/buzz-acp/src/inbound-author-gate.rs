use super::{
    effective_prompt_author, is_dm_channel, is_owner_or_sibling, pool, refresh_relay_self, relay,
    OwnerCache, RespondTo,
};
use std::collections::HashSet;

pub(crate) struct InboundAuthorGateDecision {
    pub(crate) effective_author: String,
    pub(crate) allowed: bool,
    pub(crate) is_dm: bool,
}

/// An event that passed the complete listener author boundary.
///
/// The event is moved into the gate before policy evaluation and can only
/// be recovered through this private-field capability. Both production
/// loops therefore have to consume the gate's verdict before they can use
/// or publish the event; replacing the call with a raw signer or a local
/// `allowed = true` no longer type-checks.
pub(crate) struct AuthorizedListenerEvent {
    buzz_event: relay::BuzzEvent,
    effective_author: String,
    is_dm: bool,
}

impl AuthorizedListenerEvent {
    pub(crate) fn channel_is_dm(&self) -> bool {
        self.is_dm
    }

    pub(crate) fn event(&self) -> &relay::BuzzEvent {
        &self.buzz_event
    }

    pub(crate) fn into_parts(self) -> (relay::BuzzEvent, String) {
        (self.buzz_event, self.effective_author)
    }
}

/// Apply the configured raw-author policy after trusted workflow attribution.
///
/// This stays private to the gate module so neither listener can bypass
/// workflow attribution by calling the raw-signer policy directly.
async fn author_allowed(
    respond_to: &RespondTo,
    allowlist: &HashSet<String>,
    author: &str,
    is_dm: bool,
    owner_cache: &OwnerCache,
    rest_client: &relay::RestClient,
) -> bool {
    if is_dm {
        return match respond_to {
            RespondTo::Nobody => false,
            _ => is_owner_or_sibling(author, owner_cache, rest_client).await,
        };
    }
    match respond_to {
        RespondTo::Anyone => true,
        RespondTo::Nobody => false,
        RespondTo::OwnerOnly => is_owner_or_sibling(author, owner_cache, rest_client).await,
        RespondTo::Allowlist => {
            allowlist.contains(author)
                || is_owner_or_sibling(author, owner_cache, rest_client).await
        }
    }
}

#[cfg(test)]
pub(super) async fn test_author_allowed(
    respond_to: &RespondTo,
    allowlist: &HashSet<String>,
    author: &str,
    is_dm: bool,
    owner_cache: &OwnerCache,
    rest_client: &relay::RestClient,
) -> bool {
    author_allowed(
        respond_to,
        allowlist,
        author,
        is_dm,
        owner_cache,
        rest_client,
    )
    .await
}

pub(crate) struct InboundAuthorGate {
    agent_pubkey_hex: String,
    relay_self: Option<String>,
    // None means no authoritative NIP-11 result yet, including at startup.
    refreshed_generation: Option<u64>,
}

pub(crate) fn refresh_needed(refreshed_generation: Option<u64>, event_generation: u64) -> bool {
    refreshed_generation.is_none_or(|generation| event_generation > generation)
}

impl InboundAuthorGate {
    /// Load the relay signing identity for a freshly connected listener.
    pub(crate) async fn connect(
        rest_client: &relay::RestClient,
        agent_pubkey_hex: &str,
        context: &str,
    ) -> Self {
        let (relay_self, completed) = refresh_relay_self(rest_client, None, context).await;
        Self {
            agent_pubkey_hex: agent_pubkey_hex.to_string(),
            relay_self,
            refreshed_generation: completed.then_some(0),
        }
    }

    /// Whether delegated workflow attribution is currently available.
    ///
    /// Test-only: production code never branches on this.
    /// `refresh_relay_self` already logs why attribution is unavailable, and
    /// every runtime path treats a missing identity by falling back to the
    /// raw signer.
    #[cfg(test)]
    pub(crate) fn has_relay_identity(&self) -> bool {
        self.relay_self.is_some()
    }

    #[cfg(test)]
    pub(crate) fn relay_identity_for_test(&self) -> Option<&str> {
        self.relay_self.as_deref()
    }

    /// Refresh relay identity, resolve channel trust, and apply trusted
    /// workflow attribution and author policy for one listener event.
    ///
    /// Both production listeners call this exact boundary. Identity refresh
    /// cannot be omitted independently of authorization; the raw-author
    /// policy and relay identity are private to this module.
    pub(crate) async fn evaluate_listener_event(
        &mut self,
        buzz_event: &relay::BuzzEvent,
        respond_to: &RespondTo,
        allowlist: &HashSet<String>,
        owner_cache: &OwnerCache,
        channel_info: &pool::ChannelInfoResolver,
        rest_client: &relay::RestClient,
    ) -> InboundAuthorGateDecision {
        // Retry failed startup discovery on generation 0 as well as failed
        // reconnect refreshes. Only an authoritative result completes the
        // generation; transient failure retains the last verified key.
        if refresh_needed(self.refreshed_generation, buzz_event.connection_generation) {
            let (relay_self, completed) =
                refresh_relay_self(rest_client, self.relay_self.take(), "listener").await;
            self.relay_self = relay_self;
            if completed {
                self.refreshed_generation = Some(buzz_event.connection_generation);
            }
        }
        let is_dm = is_dm_channel(buzz_event.channel_id, channel_info).await;
        self.evaluate_with_channel_trust(
            &buzz_event.event,
            respond_to,
            allowlist,
            is_dm,
            owner_cache,
            rest_client,
        )
        .await
    }

    async fn evaluate_with_channel_trust(
        &self,
        event: &nostr::Event,
        respond_to: &RespondTo,
        allowlist: &HashSet<String>,
        is_dm: bool,
        owner_cache: &OwnerCache,
        rest_client: &relay::RestClient,
    ) -> InboundAuthorGateDecision {
        let effective_author =
            effective_prompt_author(event, self.relay_self.as_deref(), &self.agent_pubkey_hex);
        let allowed = author_allowed(
            respond_to,
            allowlist,
            &effective_author,
            is_dm,
            owner_cache,
            rest_client,
        )
        .await;
        InboundAuthorGateDecision {
            effective_author,
            allowed,
            is_dm,
        }
    }
}

include!("inbound-author-authorization.rs");
