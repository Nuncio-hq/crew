#![cfg(unix)]

use super::*;
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;

#[test]
fn only_the_catalogued_native_recipes_can_reach_this_service() {
    let claude = contract_for_runtime("claude").expect("Claude is the wired adapter");
    assert_eq!(claude.command, Some("claude"));
    assert_eq!(claude.selection, RecapSelectionContract::ExplicitModel);
    let hermes = contract_for_runtime("hermes").expect("Hermes is the wired adapter");
    assert_eq!(hermes.command, Some("hermes"));
    assert_eq!(hermes.selection, RecapSelectionContract::StagingProfile);
    assert!(contract_for_runtime("codex").is_none());
}

fn admission(executable: &std::path::Path, model: &str) -> RecapAdmission {
    let fingerprint = hex::encode(Sha256::digest(std::fs::read(executable).unwrap()));
    RecapAdmission {
        runtime_id: "claude".to_string(),
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
