#![cfg(unix)]

use super::*;
use crate::managed_agents::recap_adapter::claude_recap_plan;
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;

#[test]
fn only_the_catalogued_claude_recipe_can_reach_this_service() {
    let claude = contract_for_runtime("claude").expect("Claude is the wired adapter");
    assert_eq!(claude.command, Some("claude"));
    assert_eq!(claude.selection, RecapSelectionContract::ExplicitModel);
    assert!(contract_for_runtime("hermes").is_none());
    assert!(contract_for_runtime("codex").is_none());
}

fn admission(executable: &std::path::Path, model: &str) -> RecapAdmission {
    let fingerprint = hex::encode(Sha256::digest(std::fs::read(executable).unwrap()));
    RecapAdmission {
        runtime_id: RECAP_RUNTIME_ID.to_string(),
        executable: super::super::recap_capability::RecapExecutableIdentity {
            resolved_path: executable.to_path_buf(),
            version: "fixture-1".to_string(),
            fingerprint,
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        },
        selection: super::super::recap_capability::RecapSelection {
            model: model.to_string(),
            profile: None,
            auth_available: true,
        },
    }
}

#[test]
fn executable_replacement_after_planning_is_rejected_before_spawn() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path().canonicalize().unwrap();
    let executable = base.join("fixture-claude");
    let marker = base.join("spawned");
    std::fs::write(
        &executable,
        format!(
            "#!/bin/sh\ntouch '{}'\nprintf '%s' '{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"fixture recap\",\"modelUsage\":{{\"fixture-model\":{{}}}}}}'\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let admission = admission(&executable, "fixture-model");
    let run = OwnedRecapRun::create(&base, 100).unwrap();
    let plan =
        claude_recap_plan(&executable, run.path(), "fixture-model", b"private prompt").unwrap();

    // Replace the path after planning. The old admission fingerprint must not
    // authorize this new executable, and the production seam must not spawn it.
    std::fs::write(&executable, b"#!/bin/sh\ntouch should-not-run\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();

    let result = execute_admitted_recap(
        admission,
        run,
        plan,
        b"private prompt",
        Duration::from_secs(5),
        &AtomicBool::new(false),
    );
    assert_eq!(
        result,
        Err(RecapServiceFailure::Admission(
            RecapFailure::InvalidExecutableIdentity
        ))
    );
    assert!(
        !marker.exists(),
        "replaced executable must never be spawned"
    );
}

#[test]
fn admitted_execution_uses_private_stdin_and_cleans_finished_run() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path().canonicalize().unwrap();
    let executable = base.join("fixture-claude");
    std::fs::write(
        &executable,
        b"#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"fixture recap\",\"modelUsage\":{\"fixture-model\":{}}}'\n",
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let admission = admission(&executable, "fixture-model");
    let run = OwnedRecapRun::create(&base, 100).unwrap();
    let plan =
        claude_recap_plan(&executable, run.path(), "fixture-model", b"private prompt").unwrap();
    let path = run.path().to_path_buf();
    let result = execute_admitted_recap(
        admission,
        run,
        plan,
        b"private prompt",
        Duration::from_secs(5),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(result, "fixture recap");
    assert!(!path.exists(), "finished disposable state must be removed");
}

#[test]
fn settings_default_off_and_atomic_snapshot_round_trip() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("recap-settings-v1.json");
    let settings = RecapSettings::default();
    assert_eq!(settings.mode, RecapMode::Off);
    atomic_write_json(&path, &settings).unwrap();
    assert_eq!(load_settings_from_path(&path).unwrap(), settings);
}

#[test]
fn settings_validation_rejects_manual_unsupported_runtime() {
    let settings = RecapSettings {
        mode: RecapMode::Manual,
        runtime_id: Some("claude".to_string()),
        requested_model: Some("fixture-model".to_string()),
        ..RecapSettings::default()
    };
    let runtimes = vec![RecapRuntimeOption {
        id: "claude".to_string(),
        label: "Claude".to_string(),
        kind: "cli".to_string(),
        availability: "unsupported".to_string(),
        reason: Some("runtime_not_ready".to_string()),
        capability_fingerprint: None,
        profiles: Vec::new(),
        models: Vec::new(),
    }];
    assert_eq!(
        validate_settings(&settings, &runtimes),
        Err("unsupported_runtime".to_string())
    );
}

#[test]
fn settings_validation_requires_an_inventory_model_and_profile() {
    let cli = RecapRuntimeOption {
        id: "claude".to_string(),
        label: "Claude".to_string(),
        kind: "cli".to_string(),
        availability: "supported".to_string(),
        reason: None,
        capability_fingerprint: Some("fp-1".to_string()),
        profiles: Vec::new(),
        models: vec!["certified-model".to_string()],
    };
    let invalid_model = RecapSettings {
        mode: RecapMode::Manual,
        runtime_id: Some("claude".to_string()),
        requested_model: Some("invented-model".to_string()),
        capability_fingerprint: Some("fp-1".to_string()),
        ..RecapSettings::default()
    };
    assert_eq!(
        validate_settings(&invalid_model, std::slice::from_ref(&cli)),
        Err("unsupported_model".to_string())
    );

    let hermes = RecapRuntimeOption {
        id: "hermes".to_string(),
        label: "Hermes".to_string(),
        kind: "hermes".to_string(),
        availability: "supported".to_string(),
        reason: None,
        capability_fingerprint: Some("fp-2".to_string()),
        profiles: vec![RecapProfileOption {
            id: "research".to_string(),
            label: "Research".to_string(),
        }],
        models: Vec::new(),
    };
    let invalid_profile = RecapSettings {
        mode: RecapMode::Manual,
        runtime_id: Some("hermes".to_string()),
        profile_ref: Some("missing".to_string()),
        capability_fingerprint: Some("fp-2".to_string()),
        ..RecapSettings::default()
    };
    assert_eq!(
        validate_settings(&invalid_profile, std::slice::from_ref(&hermes)),
        Err("unsupported_profile".to_string())
    );
}

#[test]
fn generation_key_and_cancel_are_scoped_to_all_three_ids() {
    let request = ThreadRecapGenerationRequest {
        channel_id: "channel".to_string(),
        root_event_id: "root".to_string(),
        generation_id: "generation".to_string(),
    };
    let key = generation_key(&request).unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    active_generations()
        .lock()
        .unwrap()
        .insert(key.clone(), cancelled.clone());
    cancelled.store(true, Ordering::Release);
    assert!(active_generations()
        .lock()
        .unwrap()
        .get(&key)
        .unwrap()
        .load(Ordering::Acquire));
    active_generations().lock().unwrap().remove(&key);
}

#[test]
fn all_recap_commands_are_registered_at_the_tauri_invoke_boundary() {
    let invoke_source = include_str!("../../invoke.rs");
    for command in [
        "get_recap_settings",
        "save_recap_settings",
        "get_thread_recap",
        "generate_thread_recap",
        "cancel_thread_recap",
    ] {
        assert!(
            invoke_source.contains(command),
            "missing registered command {command}"
        );
    }
    assert!(!invoke_source.contains("recap_service::run_recap"));
}
