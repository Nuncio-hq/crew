//! Persist a legacy local incarnation before admitting irreversible work.
use super::ManagedAgentRecord;

/// Acquire a stable local identity while the caller holds the managed-store lock.
/// Saving the harmless metadata prefix must succeed before any intent/effect.
/// Failed saves leave the caller's snapshot unchanged; retry reloads disk first.
pub(crate) fn ensure_instance_generation(
    records: &mut Vec<ManagedAgentRecord>,
    pubkey: &str,
    persist: impl FnOnce(&[ManagedAgentRecord]) -> Result<(), String>,
) -> Result<uuid::Uuid, String> {
    let index = records
        .iter()
        .position(|record| record.pubkey == pubkey)
        .ok_or_else(|| "The managed instance is no longer available".to_string())?;
    if pubkey.is_empty() {
        return Err("A definition has no managed instance identity".into());
    }
    if let Some(generation) = records[index].instance_generation {
        if generation.is_nil() {
            return Err("The managed instance identity requires review".into());
        }
        return Ok(generation);
    }
    let mut updated = records.clone();
    let generation = uuid::Uuid::new_v4();
    updated[index].instance_generation = Some(generation);
    persist(&updated)?;
    *records = updated;
    Ok(generation)
}

/// Refuse new instance work while native offboarding owns its global claim.
/// Caller holds the managed-store lock (or runtime transition lock through
/// spawn registration); never hold this database connection across an await.
pub(crate) fn assert_instance_available<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    pubkey: &str,
) -> Result<(), String> {
    let path = crate::commands::journal_path(app)?;
    let store = crate::owner_operations::OperationStore::open(
        &path,
        crate::owner_operations::Limits::default(),
    )
    .map_err(|error| error.to_string())?;
    if store
        .managed_agent_delete_is_pending(pubkey)
        .map_err(|error| error.to_string())?
    {
        return Err("Instance removal is pending. Open recovery to retry or review it".into());
    }
    Ok(())
}

/// Refuse persona mutations/cascades that affect a claimed keyed instance.
/// The caller holds the managed-store lock before loading this membership.
pub(crate) fn assert_persona_instances_available<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    persona_id: &str,
) -> Result<(), String> {
    for record in super::load_managed_agents(app)? {
        if record.persona_id.as_deref() == Some(persona_id) {
            assert_instance_available(app, &record.pubkey)?;
        }
    }
    Ok(())
}

/// Refuse team mutation/cascade before touching any claimed member.
/// Caller holds the managed-store lock.
pub(crate) fn assert_team_instances_available<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    team_id: &str,
) -> Result<(), String> {
    let teams = super::load_teams(app)?;
    let persona_ids = teams
        .iter()
        .find(|team| team.id == team_id)
        .map(|team| team.persona_ids.as_slice())
        .unwrap_or_default();
    for record in super::load_managed_agents(app)? {
        if record.team_id.as_deref() == Some(team_id)
            || record
                .persona_id
                .as_ref()
                .is_some_and(|id| persona_ids.contains(id))
        {
            assert_instance_available(app, &record.pubkey)?;
        }
    }
    Ok(())
}

/// Reuse the existing per-instance provider serialization seam for profile IO.
/// No store/process lock is held while waiting for this asynchronous guard.
pub(crate) async fn lock_instance_mutation(
    app: &tauri::AppHandle,
    pubkey: &str,
    generation: Option<uuid::Uuid>,
) -> Result<tokio::sync::OwnedMutexGuard<()>, String> {
    use tauri::Manager;
    let state = app.state::<crate::app_state::AppState>();
    let lock = {
        let mut locks = state
            .provider_deploy_locks
            .lock()
            .map_err(|_| "Instance mutation lock unavailable")?;
        std::sync::Arc::clone(
            locks
                .entry(pubkey.to_string())
                .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(()))),
        )
    };
    let guard = tokio::time::timeout(std::time::Duration::from_secs(10), lock.lock_owned())
        .await
        .map_err(|_| "Instance mutation is busy; retry")?;
    assert_instance_generation(app, pubkey, generation)?;
    Ok(guard)
}

/// Revalidate a captured incarnation under a short managed-store lock.
/// Caller must not already hold that lock. Keep its mutation guard across
/// network IO so offboarding admission cannot overtake an in-flight publish.
pub(crate) fn assert_instance_generation(
    app: &tauri::AppHandle,
    pubkey: &str,
    generation: Option<uuid::Uuid>,
) -> Result<(), String> {
    use tauri::Manager;
    let state = app.state::<crate::app_state::AppState>();
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|_| "Instance store lock unavailable")?;
    assert_instance_available(app, pubkey)?;
    let records = super::load_managed_agents(app)?;
    let record = records
        .iter()
        .find(|record| record.pubkey == pubkey)
        .ok_or("The managed instance is no longer available")?;
    if record.instance_generation != generation {
        return Err("The managed instance changed while work was in flight".into());
    }
    Ok(())
}
