use super::access::{coordinate_parts, Access};
use super::admission::{picker_bridge, spawn_source_read, Admission, Selection};
use super::grants::{Grant, GrantInfo, GrantStore};
use crew_wiki::snapshot_v1::verify_snapshot_index;
use crew_wiki::source_access::SelectedSourceRoot;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, State};

#[derive(Default)]
pub struct SourceState {
    pub(super) admission: Admission,
    pub(super) grants: GrantStore,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceContent {
    content: String,
    start_line: u64,
    end_line: u64,
}

/// Open the native folder chooser and install one scoped source capability.
#[tauri::command]
pub async fn wiki_choose_source_root(
    app: AppHandle,
    state: State<'_, SourceState>,
    repository_coordinate: String,
) -> Result<Option<GrantInfo>, String> {
    use tauri_plugin_dialog::DialogExt;

    let captured = crate::app_state::owner_scope::capture(app.clone()).await?;
    let access = super::access::NativeAccess::capture(app.clone(), captured.token).await?;
    choose(
        &state,
        &access,
        &repository_coordinate,
        move |title, callback| {
            app.dialog()
                .file()
                .set_title(title)
                .pick_folder(move |path| {
                    let selected = path.and_then(|path| path.as_path().map(ToOwned::to_owned));
                    callback(Ok(selected));
                });
        },
    )
    .await
}

/// Return the native source capabilities for the current owner/workspace scope.
#[tauri::command]
pub async fn wiki_source_grants(
    app: AppHandle,
    state: State<'_, SourceState>,
) -> Result<Vec<GrantInfo>, String> {
    let captured = crate::app_state::owner_scope::capture(app).await?;
    state.grants.list(&captured.token)
}

/// Forget one source capability without touching relay state or repository events.
#[tauri::command]
pub async fn wiki_forget_source_root(
    app: AppHandle,
    state: State<'_, SourceState>,
    capability_id: String,
) -> Result<(), String> {
    let captured = crate::app_state::owner_scope::capture(app).await?;
    state.grants.forget(&captured.token, &capability_id)
}

/// Read one authenticated source reference through a previously granted root.
#[tauri::command]
pub async fn wiki_open_verified_source(
    app: AppHandle,
    state: State<'_, SourceState>,
    capability_id: String,
    page: Value,
    reference_index: usize,
) -> Result<SourceContent, String> {
    let captured = crate::app_state::owner_scope::capture(app.clone()).await?;
    let access = super::access::NativeAccess::capture(app, captured.token).await?;
    let grant = state.grants.get(access.token(), &capability_id)?;
    let (head, manifest) = access.snapshot_inputs(&grant.anchor.coordinate).await?;
    open(
        &state,
        &access,
        &capability_id,
        &head,
        &manifest,
        &page,
        reference_index,
    )
    .await
}

pub(super) async fn choose<A: Access, P: FnOnce(String, Box<dyn FnOnce(Selection) + Send>)>(
    state: &SourceState,
    access: &A,
    coordinate: &str,
    picker: P,
) -> Result<Option<GrantInfo>, String> {
    let permit = state.admission.choose()?;
    coordinate_parts(coordinate)?;
    access.current().await?;
    let anchor = access.repository(coordinate).await?;
    let (pending, callback) = picker_bridge(permit);
    picker(
        format!("Choose source folder for {}", anchor.label),
        Box::new(callback),
    );
    let (selected, permit) = pending.wait().await?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    if access.repository(coordinate).await? != anchor {
        return Err("Repository changed while choosing a folder".into());
    }
    let root = spawn_source_read(permit.clone(), move || {
        SelectedSourceRoot::open_native_selection(&selected)
    })
    .await
    .map_err(|_| "Source folder selection failed")?
    .map_err(|error| error.to_string())?;
    access.current().await?;
    if access.repository(coordinate).await? != anchor {
        return Err("Repository changed while opening a folder".into());
    }
    state.grants.install(access.token(), anchor, root).map(Some)
}

pub(super) async fn open<A: Access>(
    state: &SourceState,
    access: &A,
    id: &str,
    head: &Value,
    manifest: &Value,
    page: &Value,
    reference_index: usize,
) -> Result<SourceContent, String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), async {
        let permit = state.admission.read()?;
        if let Err(error) = access.current().await {
            let _ = state.grants.forget(access.token(), id);
            return Err(error);
        }
        let grant = state.grants.get(access.token(), id)?;
        let (owner, repo) = coordinate_parts(&grant.anchor.coordinate)?;
        let index = verify_snapshot_index(owner, repo, head, manifest)
            .map_err(|error| error.to_string())?;
        let verified_page = index.verify_page(page).map_err(|error| error.to_string())?;
        let reference = verified_page
            .source_references()
            .get(reference_index)
            .ok_or("Source reference unavailable")?
            .clone();
        let revision = index.source_revision().to_owned();
        if !revision.starts_with(&format!("{}:", grant.anchor.workspace_mode)) {
            return Err("Source mode differs from the selected repository".into());
        }
        validate_grant(state, access, &grant).await?;
        access
            .head(&grant.anchor.coordinate, index.head_event_id())
            .await?;
        state.grants.current(access.token(), &grant)?;
        let worker_grant = Arc::clone(&grant);
        let result = spawn_source_read(permit.clone(), move || {
            worker_grant
                .root
                .read_verified_reference(&revision, &reference, deadline)
        })
        .await
        .map_err(|_| "Source read failed")?
        .map_err(|error| error.to_string())?;
        validate_grant(state, access, &grant).await?;
        access
            .head(&grant.anchor.coordinate, index.head_event_id())
            .await?;
        access.current().await?;
        state.grants.current(access.token(), &grant)?;
        let content = SourceContent {
            content: result.content,
            start_line: result.start_line,
            end_line: result.end_line,
        };
        drop(permit);
        Ok(content)
    })
    .await
    .map_err(|_| "Source read deadline reached".to_string())?
}

async fn validate_grant<A: Access>(
    state: &SourceState,
    access: &A,
    grant: &Arc<Grant>,
) -> Result<(), String> {
    access.current().await?;
    state.grants.current(access.token(), grant)?;
    match access.repository(&grant.anchor.coordinate).await {
        Ok(anchor) if anchor == grant.anchor => Ok(()),
        _ => {
            state
                .grants
                .forget(access.token(), &grant.info.capability_id)?;
            Err("Repository access or identity changed; choose the source folder again".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::owner_scope::{OwnerScopeToken, OWNER_SCOPE_STALE};
    use crate::commands::wiki_source::grants::RepositoryAnchor;
    use crate::owner_operations::OperationScope;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    const FIXTURE: &str =
        include_str!("../../../../../crates/crew-wiki/tests/fixtures/wiki-snapshot-v1-folder.json");

    struct FakeAccess {
        token: OwnerScopeToken,
        anchor: RepositoryAnchor,
        head_id: String,
        revoked: Arc<AtomicBool>,
    }
    impl FakeAccess {
        fn check(&self) -> Result<(), String> {
            if self.revoked.load(Ordering::Acquire) {
                Err(OWNER_SCOPE_STALE.into())
            } else {
                Ok(())
            }
        }
    }
    impl Access for FakeAccess {
        fn token(&self) -> &OwnerScopeToken {
            &self.token
        }
        async fn current(&self) -> Result<(), String> {
            self.check()
        }
        async fn repository(&self, coordinate: &str) -> Result<RepositoryAnchor, String> {
            self.check()?;
            if coordinate == self.anchor.coordinate {
                Ok(self.anchor.clone())
            } else {
                Err("wrong repository".into())
            }
        }
        async fn head(&self, _coordinate: &str, expected_id: &str) -> Result<(), String> {
            self.check()?;
            (expected_id == self.head_id)
                .then_some(())
                .ok_or_else(|| "wrong head".into())
        }
        async fn snapshot_inputs(&self, _coordinate: &str) -> Result<(Value, Value), String> {
            Err("fixture access does not query snapshots".into())
        }
    }

    fn fixture() -> (Value, Value, Value, String, String, OwnerScopeToken) {
        let value: Value = serde_json::from_str(FIXTURE).unwrap();
        let owner = value["owner"].as_str().unwrap().to_owned();
        let repo = value["repoD"].as_str().unwrap().to_owned();
        let head_id = value["head"]["id"].as_str().unwrap().to_owned();
        let coordinate = format!("30617:{owner}:{repo}");
        let token = OwnerScopeToken {
            scope: OperationScope {
                owner,
                community: "https://fixture.invalid".into(),
            },
            workspace_generation: 1,
            identity_generation: 1,
        };
        (
            value["head"].clone(),
            value["manifest"].clone(),
            value["pages"][0].clone(),
            coordinate,
            head_id,
            token,
        )
    }

    fn access(
        coordinate: &str,
        head_id: &str,
        token: OwnerScopeToken,
        revoked: Arc<AtomicBool>,
    ) -> FakeAccess {
        FakeAccess {
            token,
            anchor: RepositoryAnchor {
                coordinate: coordinate.into(),
                event_id: "b".repeat(64),
                workspace_mode: "folder".into(),
                label: "Repo.demo".into(),
            },
            head_id: head_id.into(),
            revoked,
        }
    }

    #[tokio::test]
    async fn choose_then_open_reads_only_the_signed_reference() {
        let (head, manifest, page, coordinate, head_id, token) = fixture();
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("nguồn")).unwrap();
        std::fs::write(directory.path().join("nguồn/file.rs"), "source\n").unwrap();
        let revoked = Arc::new(AtomicBool::new(false));
        let fake = access(&coordinate, &head_id, token.clone(), Arc::clone(&revoked));
        let state = SourceState::default();
        let selected = directory.path().to_path_buf();
        let grant = choose(&state, &fake, &coordinate, move |_title, callback| {
            callback(Ok(Some(selected)));
        })
        .await
        .unwrap()
        .unwrap();
        let source = open(
            &state,
            &fake,
            &grant.capability_id,
            &head,
            &manifest,
            &page,
            0,
        )
        .await
        .unwrap();
        assert_eq!(source.content, "source\n");
        assert_eq!((source.start_line, source.end_line), (1, 1));
    }

    #[tokio::test]
    async fn revoked_scope_discards_read_and_forgets_the_capability() {
        let (head, manifest, page, coordinate, head_id, token) = fixture();
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("nguồn")).unwrap();
        std::fs::write(directory.path().join("nguồn/file.rs"), "source\n").unwrap();
        let revoked = Arc::new(AtomicBool::new(false));
        let fake = access(&coordinate, &head_id, token.clone(), Arc::clone(&revoked));
        let state = SourceState::default();
        let selected = directory.path().to_path_buf();
        let grant = choose(&state, &fake, &coordinate, move |_title, callback| {
            callback(Ok(Some(selected)));
        })
        .await
        .unwrap()
        .unwrap();
        revoked.store(true, Ordering::Release);
        assert!(open(
            &state,
            &fake,
            &grant.capability_id,
            &head,
            &manifest,
            &page,
            0
        )
        .await
        .is_err());
        assert!(state.grants.get(&token, &grant.capability_id).is_err());
    }
}
