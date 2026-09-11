use super::*;

fn runtime_kind(id: &str) -> &'static str {
    match id {
        "hermes" => "hermes",
        "claude" | "codex" => "cli",
        _ => "unknown",
    }
}

fn canonical_relay_origin(relay: &str) -> Result<String, String> {
    let mut url = url::Url::parse(relay).map_err(|_| "invalid_relay".to_string())?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return Err("invalid_relay".to_string());
    }
    let scheme = match url.scheme() {
        "ws" | "http" => "http",
        "wss" | "https" => "https",
        _ => return Err("invalid_relay".to_string()),
    };
    url.set_scheme(scheme)
        .map_err(|_| "invalid_relay".to_string())?;
    Ok(url.origin().ascii_serialization())
}

pub(super) async fn capture_owner_scope<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> Result<
    (
        crate::app_state::owner_scope::CapturedOwnerScope,
        String,
        String,
    ),
    String,
> {
    let scope = crate::app_state::owner_scope::capture(app.clone()).await?;
    let viewer_pubkey = scope.keys.public_key().to_hex();
    let relay_origin = canonical_relay_origin(&scope.relay_url)?;
    Ok((scope, viewer_pubkey, relay_origin))
}

pub(super) fn runtime_inventory<R: tauri::Runtime>(app: &AppHandle<R>) -> Vec<RecapRuntimeOption> {
    // Runtime support is projected only from a proof that is tied to the
    // active owner/relay retention scope. A standalone staging grant is not
    // enough: the positive probe row must survive in the same durable store
    // used by the managed-agent lifecycle.
    let proof = super::super::recap_ownership::runtime_ready_proof_for_app(app).ok();

    KNOWN_ACP_RUNTIMES
        .iter()
        .map(|runtime| {
            let contract = runtime.recap_contract();
            let service_wired =
                super::super::recap_service::contract_for_runtime(runtime.id).is_some();
            let mut option = RecapRuntimeOption {
                id: runtime.id.to_string(),
                label: runtime.label.to_string(),
                kind: runtime_kind(runtime.id).to_string(),
                availability: "unsupported".to_string(),
                reason: if !service_wired {
                    Some("No native one-shot recap adapter is registered.".to_string())
                } else if contract.command.is_some() {
                    Some("A separately issued native runtime-ready grant is required.".to_string())
                } else {
                    Some("No native one-shot recap adapter is registered.".to_string())
                },
                capability_fingerprint: None,
                profiles: Vec::new(),
                models: Vec::new(),
            };
            if service_wired {
                if let Some(proof) = proof.as_ref() {
                    let requested = proof.selection_for_service();
                    if let Ok(admission) =
                        admit_runtime_ready(runtime.id, contract, proof, &requested)
                    {
                        option.availability = "supported".to_string();
                        option.reason = None;
                        option.capability_fingerprint = Some(sha256_hex(format!(
                            "v1\0{}\0{}\0{}",
                            admission.runtime_id,
                            admission.executable.fingerprint,
                            admission.selection.model,
                        )));
                        option.models.push(admission.selection.model);
                    } else if contract.command.is_some() {
                        option.reason = Some("runtime_not_ready".to_string());
                    }
                }
            }
            option
        })
        .collect()
}

fn valid_text_field(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(|value| {
        !value.is_empty()
            && value == value.trim()
            && value.len() <= MAX_TEXT_FIELD_BYTES
            && !value.chars().any(char::is_control)
    })
}

pub(super) fn validate_settings(
    settings: &RecapSettings,
    runtimes: &[RecapRuntimeOption],
) -> Result<RecapSettings, String> {
    if settings.version != 1
        || settings.bounds != default_bounds()
        || !valid_text_field(&settings.runtime_id)
        || !valid_text_field(&settings.requested_model)
        || !valid_text_field(&settings.profile_ref)
        || !valid_text_field(&settings.capability_fingerprint)
    {
        return Err("invalid_settings".to_string());
    }
    let mut normalized = settings.clone();
    if settings.mode == RecapMode::Off {
        normalized.runtime_id = None;
        normalized.requested_model = None;
        normalized.profile_ref = None;
        normalized.capability_fingerprint = None;
        return Ok(normalized);
    }
    let Some(runtime_id) = settings.runtime_id.as_deref() else {
        return Err("missing_selection".to_string());
    };
    let Some(runtime) = runtimes.iter().find(|runtime| runtime.id == runtime_id) else {
        return Err("runtime_not_ready".to_string());
    };
    if runtime.availability != "supported" {
        return Err("runtime_not_ready".to_string());
    }
    if settings.capability_fingerprint.as_deref() != runtime.capability_fingerprint.as_deref() {
        return Err("capability_changed".to_string());
    }
    if runtime.kind == "hermes" {
        if settings.profile_ref.is_none() {
            return Err("missing_selection".to_string());
        }
    } else if settings.profile_ref.is_some() {
        return Err("profile_mismatch".to_string());
    }
    let Some(model) = settings.requested_model.as_deref() else {
        return Err("missing_selection".to_string());
    };
    if !runtime.models.iter().any(|candidate| candidate == model) {
        return Err("invalid_model_selection".to_string());
    }
    Ok(normalized)
}

pub(super) fn recoverable_settings(
    settings: RecapSettings,
    runtimes: &[RecapRuntimeOption],
) -> (RecapSettings, bool) {
    match validate_settings(&settings, runtimes) {
        Ok(settings) => (settings, true),
        Err(error) if error == "invalid_settings" => (default_settings(), false),
        // Keep an otherwise well-shaped but currently unavailable selection in
        // the snapshot so the UI can select Off and persist the recovery.
        Err(_) => (settings, false),
    }
}
