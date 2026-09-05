impl InboundAuthorGate {
    pub(crate) async fn authorize_listener_event(
        &mut self,
        buzz_event: relay::BuzzEvent,
        respond_to: &RespondTo,
        allowlist: &HashSet<String>,
        owner_cache: &OwnerCache,
        channel_info: &pool::ChannelInfoResolver,
        rest_client: &relay::RestClient,
    ) -> Option<AuthorizedListenerEvent> {
        let decision = self
            .evaluate_listener_event(
                &buzz_event,
                respond_to,
                allowlist,
                owner_cache,
                channel_info,
                rest_client,
            )
            .await;
        if !decision.allowed {
            tracing::debug!(
                channel_id = %buzz_event.channel_id,
                raw_author = %buzz_event.event.pubkey.to_hex(),
                effective_author = %decision.effective_author,
                mode = %respond_to,
                is_dm = decision.is_dm,
                "inbound author gate — dropping event"
            );
            return None;
        }
        Some(AuthorizedListenerEvent {
            buzz_event,
            effective_author: decision.effective_author,
            is_dm: decision.is_dm,
        })
    }

    #[cfg(test)]
    pub(crate) async fn evaluate_for_test(
        &self,
        event: &nostr::Event,
        respond_to: &RespondTo,
        allowlist: &HashSet<String>,
        is_dm: bool,
        owner_cache: &OwnerCache,
        rest_client: &relay::RestClient,
    ) -> InboundAuthorGateDecision {
        self.evaluate_with_channel_trust(
            event,
            respond_to,
            allowlist,
            is_dm,
            owner_cache,
            rest_client,
        )
        .await
    }
}
