#![cfg(unix)]
use super::*;
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
