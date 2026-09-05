/// Return the workflow owner attributed by a relay-signed workflow message.
///
/// `buzz:workflow-owner` alone is not authority: any ordinary event author can
/// forge custom tags. Attribution is accepted only for a cryptographically
/// valid kind:9 event signed by the active relay's NIP-11 `self` key, with
/// exactly one canonical workflow marker and owner pubkey. The current agent
/// must also have exactly one canonical `buzz:workflow-mention` tag; legacy `p`
/// tags are deliberately ignored as author-gate authority because workflows
/// retain an owner `p` tag for mentions-feed compatibility.
fn verified_workflow_owner(
    event: &nostr::Event,
    relay_self: Option<&str>,
    agent_pubkey_hex: &str,
) -> Option<String> {
    if event.kind.as_u16() as u32 != KIND_STREAM_MESSAGE {
        return None;
    }

    let relay_self = nostr::PublicKey::from_hex(relay_self?).ok()?;
    if event.pubkey != relay_self || event.verify().is_err() {
        return None;
    }

    let markers: Vec<&[String]> = event
        .tags
        .iter()
        .map(|tag| tag.as_slice())
        .filter(|values| values.first().map(String::as_str) == Some("buzz:workflow"))
        .collect();
    if markers.as_slice() != [["buzz:workflow", "true"]] {
        return None;
    }

    let owners: Vec<&[String]> = event
        .tags
        .iter()
        .map(|tag| tag.as_slice())
        .filter(|values| values.first().map(String::as_str) == Some("buzz:workflow-owner"))
        .collect();
    let [owner_tag] = owners.as_slice() else {
        return None;
    };
    let [_, owner_value] = owner_tag else {
        return None;
    };
    let owner = nostr::PublicKey::from_hex(owner_value).ok()?.to_hex();
    if owner_value.as_str() != owner {
        return None;
    }

    let agent_pubkey = nostr::PublicKey::from_hex(agent_pubkey_hex).ok()?.to_hex();
    let workflow_mentions: Vec<&[String]> = event
        .tags
        .iter()
        .map(|tag| tag.as_slice())
        .filter(|values| values.first().map(String::as_str) == Some("buzz:workflow-mention"))
        .collect();
    let mut mentioned_pubkeys = HashSet::with_capacity(workflow_mentions.len());
    for mention_tag in workflow_mentions {
        let [_, mention_value] = mention_tag else {
            return None;
        };
        let mention = nostr::PublicKey::from_hex(mention_value).ok()?.to_hex();
        if mention_value.as_str() != mention || !mentioned_pubkeys.insert(mention) {
            return None;
        }
    }
    if !mentioned_pubkeys.contains(&agent_pubkey) {
        return None;
    }

    Some(owner)
}

/// Resolve the author principal used by the inbound author gate.
fn effective_prompt_author(
    event: &nostr::Event,
    relay_self: Option<&str>,
    agent_pubkey_hex: &str,
) -> String {
    verified_workflow_owner(event, relay_self, agent_pubkey_hex)
        .unwrap_or_else(|| event.pubkey.to_hex())
}

/// Refresh the relay signing identity, logging why delegated workflow
/// attribution is unavailable. A transient fetch error keeps the last verified
/// key so a reconnect blip cannot disable workflow wakes. That availability
/// tradeoff creates a bounded-by-success revocation window: a rotated-away key
/// remains trusted while NIP-11 refreshes keep failing, then is replaced or
/// cleared by the next successful response. Refresh runs at startup and before
/// authorization on a new or still-pending generation; a completed generation
/// is not refreshed again until a reconnect.
async fn refresh_relay_self(
    rest_client: &relay::RestClient,
    current: Option<String>,
    context: &str,
) -> (Option<String>, bool) {
    match rest_client.relay_self().await {
        Ok(Some(pubkey)) => (Some(pubkey), true),
        Ok(None) => {
            tracing::warn!(
                %context,
                "relay NIP-11 document has no `self` key — workflow attribution remains fail-closed"
            );
            (None, true)
        }
        Err(error) => {
            tracing::warn!(
                %context,
                %error,
                retaining_previous_identity = current.is_some(),
                "failed to refresh relay NIP-11 identity"
            );
            (current, false)
        }
    }
}
