#![cfg(unix)]
use super::*;
use crate::managed_agents::recap_capability::{
    RecapAuthBinding, RecapExecutableIdentity, RecapProbeTarget, RecapProcessObservation,
    RecapRuntimeCertification, RecapSelection, RecapStateObservation,
};
use sha2::{Digest, Sha256};
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

fn fixture() -> (tempfile::TempDir, NativeIdentity, serde_json::Value) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap();
    let uid = std::fs::metadata(&home).unwrap().uid();
    let config_base = home.join("config");
    let app_data = config_base.join("com.nuncio.crew.staging-test");
    let config_home = config_base.join("buzz-demo-staging-test");
    let nest = home.join(".buzz-demo-staging-test");
    let profiles = home.join("crew-staging-test/profiles");
    let workspaces = home.join("crew-staging-test/workspaces");
    for path in [&app_data, &config_home, &nest, &profiles, &workspaces] {
        std::fs::create_dir_all(path).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let excluded: Vec<_> = [".buzz", ".buzz-dev", ".codex", ".claude", ".hermes"]
        .iter()
        .map(|p| home.join(p))
        .chain([config_base.join("com.nuncio.crew")])
        .collect();
    let doc = serde_json::json!({"schema":"crew-staging-ownership", "version":1,
        "environment_id":"crew-staging-test", "status":"OWNERSHIP_ONLY_NOT_RUNTIME_READY",
        "mac":{"owner_uid":uid, "home":home, "build_demo_slug":"staging-test",
        "bundle_id":"com.nuncio.crew.staging-test", "keyring_service":"buzz-desktop-demo.staging-test",
        "deep_link_scheme":"buzz-demo-staging-test", "runtime_generation_allowed":false,
        "auth_references":[], "excluded_roots":excluded,
        "roots":{"app_data":app_data,"config_home":config_home,"nest":nest,"profiles":profiles,"workspaces":workspaces}}});
    let native = NativeIdentity {
        home,
        app_data,
        config_home,
        config_base,
        uid,
        slug: "staging-test".into(),
        bundle_id: "com.nuncio.crew.staging-test".into(),
        keyring_service: "buzz-desktop-demo.staging-test".into(),
        scheme: "buzz-demo-staging-test".into(),
    };
    (temp, native, doc)
}

fn write(native: &NativeIdentity, doc: &serde_json::Value) {
    let path = native.app_data.join(OWNERSHIP_FILENAME);
    std::fs::write(&path, serde_json::to_vec(doc).unwrap()).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

fn write_runtime_grant(
    native: &NativeIdentity,
    doc: &serde_json::Value,
    executable: &std::path::Path,
    guarantees: serde_json::Value,
) {
    let ownership_bytes = serde_json::to_vec(doc).unwrap();
    let executable_bytes = std::fs::read(executable).unwrap();
    let grant = serde_json::json!({
        "schema": "crew-staging-runtime-ready",
        "version": 1,
        "environment_id": "crew-staging-test",
        "ownership_sha256": hex::encode(Sha256::digest(&ownership_bytes)),
        "status": "RUNTIME_READY",
        "owner_uid": native.uid,
        "home": native.home,
        "app_data": native.app_data,
        "bundle_id": native.bundle_id,
        "runtime_id": "claude",
        "executable": {
            "resolved_path": executable,
            "version": "fixture-1",
            "fingerprint": hex::encode(Sha256::digest(&executable_bytes)),
            "platform": format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        },
        "selection": {"model": "fixture-model", "profile": null},
        "auth_reference": "staging-auth-reference",
        "auth_service": native.keyring_service,
        "effective_model": "fixture-model",
        "output_digest": "a".repeat(64),
        "tool_probe_digest": "b".repeat(64),
        "guarantees": guarantees,
    });
    let path = native.app_data.join("crew-staging-runtime-ready-v1.json");
    std::fs::write(&path, serde_json::to_vec(&grant).unwrap()).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

fn certification(executable: &std::path::Path, auth_service: &str) -> RecapRuntimeCertification {
    let executable_bytes = std::fs::read(executable).unwrap();
    let plan = super::super::recap_adapter::claude_recap_plan(
        std::path::Path::new("/staging/claude"),
        std::path::Path::new("/staging/recap-runs/probe"),
        "fixture-model",
        b"probe",
    )
    .unwrap();
    let envelope = serde_json::json!({
        "type": "recap_probe",
        "result": "fixture recap output",
        "effectiveModel": "fixture-model",
        "oneShotCompleted": true,
        "toolProbe": {
            "probeId": "crew-recap-hostile-tool-v1",
            "toolName": "context_engine",
            "requestObserved": true,
            "deniedBeforeEffect": true,
            "sentinelBefore": "a".repeat(64),
            "sentinelAfter": "a".repeat(64)
        }
    });
    let adapter = plan
        .parse_probe_output(true, &serde_json::to_vec(&envelope).unwrap(), b"")
        .unwrap();
    RecapRuntimeCertification::from_adapter_observation(
        RecapProbeTarget {
            contract: super::super::known_acp_runtime_exact("claude")
                .unwrap()
                .recap_contract(),
            runtime_id: "claude".into(),
            executable: RecapExecutableIdentity {
                resolved_path: executable.to_owned(),
                version: "fixture-1".into(),
                fingerprint: hex::encode(Sha256::digest(&executable_bytes)),
                platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            },
            selection: RecapSelection {
                model: "fixture-model".into(),
                profile: None,
                auth_available: true,
            },
            auth: RecapAuthBinding {
                service: auth_service.into(),
                reference: "staging-auth-reference".into(),
            },
        },
        adapter,
        RecapStateObservation::Unchanged,
        RecapProcessObservation::ReapedAndContained,
    )
    .unwrap()
}

#[test]
fn receipt_resolves_only_native_owned_recap_parent() {
    let (_temp, native, doc) = fixture();
    write(&native, &doc);
    let expected = native.app_data.join("agents");
    let receipt = VerifiedStagingOwnership::from_native(native).unwrap();
    assert_eq!(receipt.recap_base().unwrap(), expected);
    assert!(
        !expected.exists(),
        "read-only ownership validation creates no runtime state"
    );
}

#[test]
fn ownership_document_cannot_claim_generation_or_credentials() {
    for field in [
        "generation",
        "auth",
        "uid",
        "bundle",
        "exclusions",
        "profile",
    ] {
        let (_temp, native, mut doc) = fixture();
        match field {
            "generation" => doc["mac"]["runtime_generation_allowed"] = true.into(),
            "auth" => doc["mac"]["auth_references"] = serde_json::json!(["not-authority"]),
            "uid" => doc["mac"]["owner_uid"] = (native.uid + 1).into(),
            "bundle" => doc["mac"]["bundle_id"] = "com.nuncio.crew".into(),
            "exclusions" => doc["mac"]["excluded_roots"] = serde_json::json!([]),
            "profile" => {
                doc["mac"]["roots"]["profiles"] = native
                    .home
                    .join(".hermes")
                    .to_string_lossy()
                    .as_ref()
                    .into()
            }
            _ => unreachable!(),
        }
        write(&native, &doc);
        assert!(
            matches!(
                VerifiedStagingOwnership::from_native(native),
                Err(RecapStateFailure::Ownership)
            ),
            "{field}"
        );
    }
}

#[test]
fn loader_rejects_symlink_manifest_shared_roots_and_oversized_json() {
    for kind in ["symlink", "shared", "oversized"] {
        let (_temp, native, doc) = fixture();
        write(&native, &doc);
        let path = native.app_data.join(OWNERSHIP_FILENAME);
        match kind {
            "symlink" => {
                let other = native.app_data.join("elsewhere");
                std::fs::rename(&path, &other).unwrap();
                symlink(other, path).unwrap();
            }
            "shared" => std::fs::set_permissions(
                &native.config_home,
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap(),
            "oversized" => std::fs::write(path, vec![b' '; 16385]).unwrap(),
            _ => unreachable!(),
        }
        assert!(
            VerifiedStagingOwnership::from_native(native).is_err(),
            "{kind}"
        );
    }
}

#[test]
fn receipt_revalidates_roots_before_projection() {
    let (_temp, native, doc) = fixture();
    write(&native, &doc);
    let root = native.config_home.clone();
    let receipt = VerifiedStagingOwnership::from_native(native).unwrap();
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(receipt.recap_base(), Err(RecapStateFailure::Ownership));
}

#[test]
fn receipt_rejects_private_directory_replacement() {
    let (_temp, native, doc) = fixture();
    write(&native, &doc);
    let root = native.config_home.clone();
    let receipt = VerifiedStagingOwnership::from_native(native).unwrap();
    std::fs::rename(&root, root.with_extension("old")).unwrap();
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(receipt.recap_base(), Err(RecapStateFailure::Ownership));
}

#[test]
fn receipt_rejects_intermediate_agents_symlink() {
    let (_temp, native, doc) = fixture();
    let outside = tempfile::tempdir().unwrap();
    write(&native, &doc);
    symlink(outside.path(), native.app_data.join("agents")).unwrap();
    let receipt = VerifiedStagingOwnership::from_native(native).unwrap();
    assert_eq!(receipt.recap_base(), Err(RecapStateFailure::Ownership));
    assert!(!outside.path().join("recap-runs").exists());
}

#[test]
fn runtime_ready_grant_is_bound_to_owned_receipt_and_current_executable() {
    let (_temp, native, doc) = fixture();
    write(&native, &doc);
    let executable = native.home.join("fixture-claude");
    std::fs::write(&executable, b"fixture executable").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    write_runtime_grant(
        &native,
        &doc,
        &executable,
        serde_json::json!({
            "one_shot": true,
            "tool_isolation": true,
            "state_isolation": true,
            "process_containment": true,
        }),
    );

    let receipt = VerifiedStagingOwnership::from_native(native).unwrap();
    assert!(receipt.runtime_ready_proof().is_ok());

    std::fs::write(&executable, b"replaced executable").unwrap();
    assert_eq!(
        receipt.runtime_ready_proof(),
        Err(RecapStateFailure::RuntimeNotReady)
    );
}

#[test]
fn native_producer_projects_only_verified_probe_and_scoped_retention_row() {
    let (_temp, native, doc) = fixture();
    write(&native, &doc);
    let executable = native.home.join("fixture-claude");
    std::fs::write(&executable, b"fixture executable").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let receipt = VerifiedStagingOwnership::from_native(native).unwrap();
    let store_path = receipt.app_data.join("retention.db");
    let store = super::super::retention::open_retention_db(&store_path).unwrap();
    let cert = certification(&executable, &receipt.native.keyring_service);

    receipt
        .issue_runtime_ready_grant(&store, &cert, 1234)
        .expect("verified probe should project the native grant");
    assert!(receipt.runtime_ready_proof_with_store(&store).is_ok());

    let grant_path = receipt.app_data.join("crew-staging-runtime-ready-v1.json");
    let grant = std::fs::read_to_string(&grant_path).unwrap();
    assert!(grant.contains("\"effective_model\":\"fixture-model\""));
    assert!(grant.contains("\"tool_probe_digest\":"));
    assert_eq!(
        std::fs::metadata(&grant_path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    store
        .execute(
            "UPDATE recap_runtime_certifications SET tool_probe_digest = ?1",
            ["c".repeat(64)],
        )
        .unwrap();
    assert_eq!(
        receipt.runtime_ready_proof_with_store(&store),
        Err(RecapStateFailure::RuntimeNotReady)
    );
}

#[test]
fn producer_rejects_mutated_executable_and_auth_binding_without_replacing_grant() {
    let (_temp, native, doc) = fixture();
    write(&native, &doc);
    let executable = native.home.join("fixture-claude");
    std::fs::write(&executable, b"fixture executable").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let receipt = VerifiedStagingOwnership::from_native(native).unwrap();
    let store_path = receipt.app_data.join("retention.db");
    let store = super::super::retention::open_retention_db(&store_path).unwrap();
    let cert = certification(&executable, &receipt.native.keyring_service);
    receipt
        .issue_runtime_ready_grant(&store, &cert, 1234)
        .unwrap();
    let grant_path = receipt.app_data.join("crew-staging-runtime-ready-v1.json");
    let before = std::fs::read(&grant_path).unwrap();

    let bad_auth = certification(&executable, "other-keyring-service");
    assert_eq!(
        receipt.issue_runtime_ready_grant(&store, &bad_auth, 1235),
        Err(RecapStateFailure::RuntimeNotReady)
    );
    assert_eq!(std::fs::read(&grant_path).unwrap(), before);

    std::fs::write(&executable, b"replacement").unwrap();
    assert_eq!(
        receipt.issue_runtime_ready_grant(&store, &cert, 1235),
        Err(RecapStateFailure::RuntimeNotReady)
    );
    assert_eq!(std::fs::read(&grant_path).unwrap(), before);
}
