struct AuthorizedNormalListenerEvent(AuthorizedListenerEvent);

struct NormalListenerIngress {
    buzz_event: relay::BuzzEvent,
    effective_author: String,
    prompt_tag: String,
    is_dm: bool,
}

impl AuthorizedNormalListenerEvent {
    async fn match_subscription(
        self,
        rules: &[SubscriptionRule],
        agent_pubkey_hex: &str,
    ) -> Option<NormalListenerIngress> {
        let is_dm = self.0.channel_is_dm();
        let (buzz_event, effective_author) = self.0.into_parts();
        let matched = filter::match_event(
            &buzz_event.event,
            buzz_event.channel_id,
            rules,
            agent_pubkey_hex,
        )
        .await?;
        Some(NormalListenerIngress {
            buzz_event,
            effective_author,
            prompt_tag: matched.prompt_tag,
            is_dm,
        })
    }
}

struct QueuedNormalListenerEvent {
    accepted: bool,
    channel_id: Uuid,
    routing_channel_id: Uuid,
    effective_author: String,
    event_id_hex: String,
    event_for_steer: nostr::Event,
    prompt_tag_for_steer: String,
}

impl QueuedNormalListenerEvent {
    fn mark_seen(&self, rest_client: &relay::RestClient) {
        if !self.accepted {
            return;
        }
        let rest_client = rest_client.clone();
        let event_id = self.event_id_hex.clone();
        tokio::spawn(async move {
            pool::reaction_add(&rest_client, &event_id, "👀").await;
        });
    }

    fn steer_or_interrupt(
        self,
        handling: MultipleEventHandling,
        owner: Option<&str>,
        pool: &mut AgentPool,
        queue: &mut EventQueue,
        steer_ack_tx: &mpsc::UnboundedSender<SteerAckEvent>,
    ) {
        if !self.accepted || !queue.is_channel_in_flight(self.channel_id) {
            return;
        }
        let Some(signal) = mode_gate_signal(handling, &self.effective_author, owner) else {
            return;
        };
        let native_attempted = matches!(signal, ControlSignal::Steer)
            && try_native_steer(
                pool,
                queue,
                self.channel_id,
                self.routing_channel_id,
                self.event_for_steer,
                self.prompt_tag_for_steer,
                steer_ack_tx,
            );
        if !native_attempted {
            signal_in_flight_task(pool, self.channel_id, self.routing_channel_id, signal);
        }
    }
}

impl NormalListenerIngress {
    fn push(
        self,
        queue: &mut EventQueue,
        hold_exempt: bool,
    ) -> QueuedNormalListenerEvent {
        let Self {
            buzz_event,
            effective_author,
            prompt_tag,
            is_dm,
        } = self;
        // Use the same resolved channel trust that admitted this event. An
        // earlier metadata miss during raw owner-control checks may be stale.
        let channel_id = conversation::id_for_event(buzz_event.channel_id, &buzz_event.event, is_dm);
        let event_id_hex = buzz_event.event.id.to_hex();
        let event_for_steer = buzz_event.event.clone();
        let prompt_tag_for_steer = prompt_tag.clone();
        let routing_channel_id = buzz_event.channel_id;
        let accepted = queue.push(QueuedEvent {
            channel_id,
            event: buzz_event.event,
            received_at: std::time::Instant::now(),
            prompt_tag,
            edited_content: None,
            hold_exempt,
        });
        QueuedNormalListenerEvent {
            accepted,
            channel_id,
            routing_channel_id,
            effective_author,
            event_id_hex,
            event_for_steer,
            prompt_tag_for_steer,
        }
    }
}

/// Apply the complete normal-listener author boundary for one relay event.
///
/// The event is consumed here, so the production loop cannot recover it except
/// from the gate's private authorized capability.
async fn authorize_normal_listener_event(
    author_gate: &mut InboundAuthorGate,
    buzz_event: relay::BuzzEvent,
    respond_to: &RespondTo,
    allowlist: &HashSet<String>,
    owner_cache: &OwnerCache,
    channel_info: &pool::ChannelInfoResolver,
    rest_client: &relay::RestClient,
) -> Option<AuthorizedListenerEvent> {
    author_gate
        .authorize_listener_event(
            buzz_event,
            respond_to,
            allowlist,
            owner_cache,
            channel_info,
            rest_client,
        )
        .await
}
