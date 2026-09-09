//! Process-local file permission, scoped to the native viewer and live repository.
use crate::app_state::owner_scope::OwnerScopeToken;
use crew_wiki::source_access::SelectedSourceRoot;
use serde::Serialize;
use std::sync::{Arc, Mutex};

const MAX_GRANTS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RepositoryAnchor {
    pub coordinate: String,
    pub event_id: String,
    pub workspace_mode: String,
    pub label: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantInfo {
    pub capability_id: String,
    pub repository_coordinate: String,
    pub token: OwnerScopeToken,
    pub label: String,
    pub workspace_mode: String,
}

pub(super) struct Grant {
    pub info: GrantInfo,
    pub anchor: RepositoryAnchor,
    pub root: SelectedSourceRoot,
}

#[derive(Default)]
pub(super) struct GrantStore(Mutex<StoreState>);

#[derive(Default)]
struct StoreState {
    token: Option<OwnerScopeToken>,
    entries: Vec<Arc<Grant>>,
}
impl StoreState {
    fn scope(&mut self, token: &OwnerScopeToken) -> Result<(), String> {
        if let Some(previous) = &self.token {
            if previous == token {
                return Ok(());
            }
            if token.identity_generation < previous.identity_generation
                || token.workspace_generation < previous.workspace_generation
                || (token.identity_generation == previous.identity_generation
                    && token.workspace_generation == previous.workspace_generation)
            {
                return Err("Source permission request belongs to an old scope".into());
            }
        }
        self.entries.clear();
        self.token = Some(token.clone());
        Ok(())
    }
}

impl GrantStore {
    pub(super) fn list(&self, token: &OwnerScopeToken) -> Result<Vec<GrantInfo>, String> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| "Source permissions unavailable")?;
        state.scope(token)?;
        Ok(state
            .entries
            .iter()
            .map(|grant| grant.info.clone())
            .collect())
    }

    pub(super) fn install(
        &self,
        token: &OwnerScopeToken,
        anchor: RepositoryAnchor,
        root: SelectedSourceRoot,
    ) -> Result<GrantInfo, String> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| "Source permissions unavailable")?;
        state.scope(token)?;
        let entries = &mut state.entries;
        let old = entries
            .iter()
            .position(|grant| grant.anchor.coordinate == anchor.coordinate);
        if old.is_none() && entries.len() >= MAX_GRANTS {
            return Err(
                "Source folder limit reached; forget a folder before choosing another".into(),
            );
        }
        let info = GrantInfo {
            capability_id: uuid::Uuid::new_v4().to_string(),
            repository_coordinate: anchor.coordinate.clone(),
            token: token.clone(),
            label: anchor.label.clone(),
            workspace_mode: anchor.workspace_mode.clone(),
        };
        let grant = Arc::new(Grant {
            info: info.clone(),
            anchor,
            root,
        });
        if let Some(index) = old {
            entries[index] = grant;
        } else {
            entries.push(grant);
        }
        Ok(info)
    }

    pub(super) fn get(&self, token: &OwnerScopeToken, id: &str) -> Result<Arc<Grant>, String> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| "Source permissions unavailable")?;
        state.scope(token)?;
        let entries = &mut state.entries;
        entries
            .iter()
            .find(|grant| grant.info.capability_id == id)
            .cloned()
            .ok_or_else(|| "Source folder permission unavailable; choose the folder again".into())
    }

    pub(super) fn current(
        &self,
        token: &OwnerScopeToken,
        grant: &Arc<Grant>,
    ) -> Result<(), String> {
        let current = self.get(token, &grant.info.capability_id)?;
        if !Arc::ptr_eq(&current, grant) {
            return Err("Source folder permission changed".into());
        }
        Ok(())
    }

    pub(super) fn forget(&self, token: &OwnerScopeToken, id: &str) -> Result<(), String> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| "Source permissions unavailable")?;
        state.scope(token)?;
        state.entries.retain(|grant| grant.info.capability_id != id);
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::owner_operations::OperationScope;

    fn token(generation: u64) -> OwnerScopeToken {
        OwnerScopeToken {
            scope: OperationScope {
                owner: "a".repeat(64),
                community: "https://fixture.invalid".into(),
            },
            workspace_generation: generation,
            identity_generation: generation,
        }
    }
    fn anchor(index: usize) -> RepositoryAnchor {
        RepositoryAnchor {
            coordinate: format!("30617:{}:repo-{index}", "a".repeat(64)),
            event_id: "b".repeat(64),
            workspace_mode: "folder".into(),
            label: format!("repo-{index}"),
        }
    }
    fn root(dir: &tempfile::TempDir) -> SelectedSourceRoot {
        SelectedSourceRoot::open_native_selection(dir.path()).unwrap()
    }

    #[test]
    fn replacement_forget_and_restart_revoke_old_ids() {
        let dir = tempfile::tempdir().unwrap();
        let store = GrantStore::default();
        let token = token(1);
        let first = store.install(&token, anchor(0), root(&dir)).unwrap();
        let running = store.get(&token, &first.capability_id).unwrap();
        let second = store.install(&token, anchor(0), root(&dir)).unwrap();
        assert!(store.current(&token, &running).is_err());
        assert!(store.get(&token, &second.capability_id).is_ok());
        assert!(GrantStore::default()
            .get(&token, &second.capability_id)
            .is_err());
        store.forget(&token, &second.capability_id).unwrap();
        assert!(store.get(&token, &second.capability_id).is_err());
    }

    #[test]
    fn aba_and_delayed_old_requests_do_not_resurrect_or_delete_new_grants() {
        let dir = tempfile::tempdir().unwrap();
        let store = GrantStore::default();
        let before = token(1);
        let after = token(3);
        let first = store.install(&before, anchor(0), root(&dir)).unwrap();
        let current = store.install(&after, anchor(0), root(&dir)).unwrap();
        assert!(store.get(&after, &first.capability_id).is_err());
        assert!(store.get(&before, &first.capability_id).is_err());
        assert!(store.forget(&before, &current.capability_id).is_err());
        assert!(store.install(&before, anchor(1), root(&dir)).is_err());
        assert!(store.get(&after, &current.capability_id).is_ok());
    }

    #[test]
    fn capacity_refuses_without_eviction_but_allows_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let store = GrantStore::default();
        let token = token(1);
        let ids: Vec<_> = (0..MAX_GRANTS)
            .map(|i| store.install(&token, anchor(i), root(&dir)).unwrap())
            .collect();
        assert!(store
            .install(&token, anchor(MAX_GRANTS), root(&dir))
            .is_err());
        for info in &ids {
            assert!(store.get(&token, &info.capability_id).is_ok());
        }
        assert!(store.install(&token, anchor(0), root(&dir)).is_ok());
        assert!(store.get(&token, "/renderer/path").is_err());
    }
}
