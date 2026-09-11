//! Owner/community/repository scoped Wiki runtime preferences.
//!
//! Wiki generation owns this small settings store separately from managed
//! employee agents and Thread Recap.  The renderer supplies only a repository
//! coordinate and a captured owner scope; the native side derives the storage
//! key and validates the runtime selection before any write.

use super::owner_operations::ScopedOperationResult;
use crate::app_state::owner_scope::{assert_current, OwnerScopeToken};
use crate::managed_agents::wiki_runtime::WikiRuntimeSelection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

const SETTINGS_VERSION: u32 = 1;
const SETTINGS_DIR: &str = "wiki-runtime-settings";
const SETTINGS_BYTES_LIMIT: u64 = 64 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredWikiRuntimeSelection {
    version: u32,
    owner: String,
    community: String,
    coordinate: String,
    selection: WikiRuntimeSelection,
}

fn validate_coordinate(expected: &OwnerScopeToken, coordinate: &str) -> Result<(), String> {
    let mut parts = coordinate.splitn(3, ':');
    let kind = parts.next().unwrap_or_default();
    let owner = parts.next().unwrap_or_default();
    let repo_d = parts.next().unwrap_or_default();
    if kind != "30617"
        || owner != expected.scope.owner
        || repo_d.is_empty()
        || coordinate.chars().any(char::is_control)
    {
        return Err("Wiki runtime settings coordinate is outside the active owner scope.".into());
    }
    Ok(())
}

fn settings_path<R: Runtime>(
    app: &AppHandle<R>,
    expected: &OwnerScopeToken,
    coordinate: &str,
) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|_| "Wiki runtime settings storage unavailable.")?;
    let directory = base.join(SETTINGS_DIR);
    std::fs::create_dir_all(&directory)
        .map_err(|_| "Wiki runtime settings storage unavailable.")?;
    #[cfg(unix)]
    std::fs::set_permissions(
        &directory,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .map_err(|_| "Wiki runtime settings storage unavailable.")?;
    let key = format!(
        "v{SETTINGS_VERSION}\0{}\0{}\0{coordinate}",
        expected.scope.owner, expected.scope.community
    );
    let digest = Sha256::digest(key.as_bytes());
    Ok(directory.join(format!("{}.json", hex::encode(digest))))
}

fn load_from_path(
    path: &Path,
    expected: &OwnerScopeToken,
    coordinate: &str,
) -> Result<Option<WikiRuntimeSelection>, String> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Wiki runtime settings could not be read.".into()),
    };
    if metadata.len() > SETTINGS_BYTES_LIMIT {
        return Err("Wiki runtime settings exceed their size limit.".into());
    }
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Wiki runtime settings could not be read.".into()),
    };
    let mut bytes = Vec::new();
    file.take(SETTINGS_BYTES_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Wiki runtime settings could not be read.".to_string())?;
    if bytes.len() as u64 > SETTINGS_BYTES_LIMIT {
        return Err("Wiki runtime settings exceed their size limit.".into());
    }
    let stored: StoredWikiRuntimeSelection = serde_json::from_slice(&bytes)
        .map_err(|_| "Wiki runtime settings are invalid; preserving the file.".to_string())?;
    if stored.version != SETTINGS_VERSION
        || stored.owner != expected.scope.owner
        || stored.community != expected.scope.community
        || stored.coordinate != coordinate
    {
        return Err("Wiki runtime settings belong to a different scope.".into());
    }
    stored
        .selection
        .validate()
        .map_err(|error| format!("Stored Wiki runtime selection is invalid: {error}"))?;
    Ok(Some(stored.selection))
}

fn write_to_path(
    path: &Path,
    expected: &OwnerScopeToken,
    coordinate: &str,
    selection: &WikiRuntimeSelection,
) -> Result<(), String> {
    let payload = serde_json::to_vec_pretty(&StoredWikiRuntimeSelection {
        version: SETTINGS_VERSION,
        owner: expected.scope.owner.clone(),
        community: expected.scope.community.clone(),
        coordinate: coordinate.to_owned(),
        selection: selection.clone(),
    })
    .map_err(|_| "Wiki runtime settings could not be encoded.".to_string())?;
    if payload.len() as u64 > SETTINGS_BYTES_LIMIT {
        return Err("Wiki runtime settings exceed their size limit.".into());
    }
    crate::managed_agents::atomic_write_json_restricted(path, &payload)
        .map_err(|_| "Wiki runtime settings could not be saved.".to_string())
}

/// Resolve the setting captured by a native Wiki job. An explicit selection
/// wins for a one-shot request; an omitted selection loads the persisted
/// repository preference in the same owner/community scope. Missing state is
/// an actionable error and never selects the heuristic generator.
pub(crate) async fn resolve_wiki_runtime_selection<R: Runtime>(
    app: AppHandle<R>,
    expected: &OwnerScopeToken,
    coordinate: &str,
    supplied: Option<WikiRuntimeSelection>,
) -> Result<WikiRuntimeSelection, String> {
    // Validate and fence the coordinate even for a one-shot explicit choice.
    // Otherwise a caller could bypass the repository binding simply by
    // supplying the selection inline instead of loading it from disk.
    validate_coordinate(expected, coordinate)?;
    assert_current(app.clone(), expected).await?;
    if let Some(selection) = supplied {
        selection
            .validate()
            .map_err(|error| format!("Invalid Wiki runtime selection: {error}"))?;
        return Ok(selection);
    }
    let path = settings_path(&app, expected, coordinate)?;
    let expected_for_read = expected.clone();
    let coordinate_for_read = coordinate.to_owned();
    let selection = tokio::task::spawn_blocking(move || {
        load_from_path(&path, &expected_for_read, &coordinate_for_read)
    })
    .await
    .map_err(|_| "Wiki runtime settings read failed.".to_string())??;
    assert_current(app, expected).await?;
    selection.ok_or_else(|| {
        "Wiki generation requires an installed runtime selection; configure one in Wiki settings."
            .to_string()
    })
}

/// Read the Wiki-only runtime/profile/model preference for one repository.
#[tauri::command]
pub(crate) async fn wiki_runtime_settings_get<R: Runtime>(
    app: AppHandle<R>,
    expected: OwnerScopeToken,
    coordinate: String,
) -> Result<ScopedOperationResult<Option<WikiRuntimeSelection>>, String> {
    validate_coordinate(&expected, &coordinate)?;
    assert_current(app.clone(), &expected).await?;
    let path = settings_path(&app, &expected, &coordinate)?;
    let expected_for_read = expected.clone();
    let coordinate_for_read = coordinate.clone();
    let value = tokio::task::spawn_blocking(move || {
        load_from_path(&path, &expected_for_read, &coordinate_for_read)
    })
    .await
    .map_err(|_| "Wiki runtime settings read failed.".to_string())??;
    assert_current(app, &expected).await?;
    Ok(ScopedOperationResult {
        token: expected,
        value,
    })
}

/// Persist a Wiki-only runtime/profile/model preference atomically.
#[tauri::command]
pub(crate) async fn wiki_runtime_settings_set<R: Runtime>(
    app: AppHandle<R>,
    expected: OwnerScopeToken,
    coordinate: String,
    selection: WikiRuntimeSelection,
) -> Result<ScopedOperationResult<WikiRuntimeSelection>, String> {
    validate_coordinate(&expected, &coordinate)?;
    selection
        .validate()
        .map_err(|error| format!("Invalid Wiki runtime selection: {error}"))?;
    assert_current(app.clone(), &expected).await?;
    let path = settings_path(&app, &expected, &coordinate)?;
    let value = selection.clone();
    let expected_for_write = expected.clone();
    let coordinate_for_write = coordinate.clone();
    tokio::task::spawn_blocking(move || {
        write_to_path(&path, &expected_for_write, &coordinate_for_write, &value)
    })
    .await
    .map_err(|_| "Wiki runtime settings write failed.".to_string())??;
    assert_current(app, &expected).await?;
    Ok(ScopedOperationResult {
        token: expected,
        value: selection,
    })
}

/// Remove one repository preference, restoring the explicit-selection state.
#[tauri::command]
pub(crate) async fn wiki_runtime_settings_clear<R: Runtime>(
    app: AppHandle<R>,
    expected: OwnerScopeToken,
    coordinate: String,
) -> Result<ScopedOperationResult<()>, String> {
    validate_coordinate(&expected, &coordinate)?;
    assert_current(app.clone(), &expected).await?;
    let path = settings_path(&app, &expected, &coordinate)?;
    tokio::task::spawn_blocking(move || match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Wiki runtime settings could not be cleared.".to_string()),
    })
    .await
    .map_err(|_| "Wiki runtime settings clear failed.".to_string())??;
    assert_current(app, &expected).await?;
    Ok(ScopedOperationResult {
        token: expected,
        value: (),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::owner_operations::OperationScope;

    fn expected(owner: char, community: &str) -> OwnerScopeToken {
        OwnerScopeToken {
            scope: OperationScope {
                owner: owner.to_string().repeat(64),
                community: community.to_string(),
            },
            workspace_generation: 1,
            identity_generation: 1,
        }
    }

    #[test]
    fn coordinate_validation_binds_the_repository_to_the_native_owner() {
        let scope = expected('a', "https://relay.example");
        assert!(validate_coordinate(&scope, &format!("30617:{}:repo", scope.scope.owner)).is_ok());
        assert!(validate_coordinate(&scope, "30617:bbbb:repo").is_err());
        assert!(validate_coordinate(&scope, &format!("30023:{}:repo", scope.scope.owner)).is_err());
    }

    #[test]
    fn stored_selection_roundtrips_and_rejects_a_different_scope() {
        let first = expected('a', "https://relay.example");
        let second = expected('b', "https://relay.example");
        let coordinate = format!("30617:{}:repo", first.scope.owner);
        let selection = WikiRuntimeSelection {
            runtime_id: "hermes".into(),
            model: None,
            profile: Some("wiki-proof".into()),
        };
        let directory = tempfile::tempdir().expect("settings directory");
        let path = directory.path().join("selection.json");
        write_to_path(&path, &first, &coordinate, &selection).expect("save");
        assert_eq!(
            load_from_path(&path, &first, &coordinate).expect("load"),
            Some(selection)
        );
        assert!(load_from_path(&path, &second, &coordinate).is_err());
    }
}
