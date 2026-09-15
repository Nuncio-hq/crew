//! Shared native Wiki generation worker wrapper.
//!
//! The publication command owns the journal lifecycle. This module owns the
//! bounded blocking call and passes a verified previous publication through to
//! the desktop Wiki worker for incremental reuse.

use super::wiki_generation_record::GenerationControl;
use crate::app_state::owner_scope::OwnerScopeToken;
use crate::managed_agents::wiki_runtime::WikiRuntimeSelection;
use crate::wiki_worker::WikiGeneration;
use crew_wiki::snapshot_v1_build::SnapshotPublication;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

/// Inputs shared by initial prepare and explicit regeneration.
pub(super) struct NativeWikiGenerationContext {
    pub(super) control: GenerationControl,
    pub(super) previous: Option<SnapshotPublication>,
}

/// Run one bounded native Wiki generation worker.
pub(super) async fn generate_native_wiki(
    expected: &OwnerScopeToken,
    owner: &str,
    repo_d: &str,
    repo_path: Option<&str>,
    workspace_mode: Option<&str>,
    runtime_selection: WikiRuntimeSelection,
    context: NativeWikiGenerationContext,
) -> Result<WikiGeneration, String> {
    let NativeWikiGenerationContext { control, previous } = context;
    let generation_owner = owner.to_owned();
    let generation_repo = repo_d.to_owned();
    let generation_path = repo_path.map(str::to_owned);
    let generation_mode = workspace_mode.map(str::to_owned);
    let generation_scope = expected.scope.community.clone();
    let generation_permit = match control.permit {
        Some(permit) => permit,
        None => super::wiki_generation_record::admit_generation().await?,
    };
    let generation_key = control.key;
    let registered_cancel = control.cancel;
    let result = tokio::task::spawn_blocking(move || {
        let _generation_permit = generation_permit;
        let _guard = crate::wiki_worker::generate_lock()
            .acquire(&format!(
                "{generation_scope}:{generation_owner}:{generation_repo}"
            ))
            .map_err(|error| error.to_string())?;
        // Initial generation already owns its unique journal claim and token.
        // Legacy regeneration registers here, after acquiring the repository
        // lock, so a rejected concurrent request cannot take over its token.
        let owns_registration = registered_cancel.is_none();
        let generation_cancel = match registered_cancel {
            Some(token) => token,
            None => crate::wiki_worker::begin_generation_cancel(&generation_key)?,
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            crate::wiki_worker::generate_wiki_pages_with_runtime_and_cancel_and_previous(
                &generation_owner,
                &generation_repo,
                generation_path.as_deref(),
                generation_mode.as_deref(),
                Some(runtime_selection),
                Some(generation_cancel.clone()),
                previous.as_ref(),
            )
            .map_err(|error| error.to_string())
        }));
        if owns_registration {
            crate::wiki_worker::finish_generation_cancel(&generation_key, &generation_cancel);
        }
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    })
    .await
    .map_err(|_| "Wiki generation worker failed.".to_string());
    result?
}
