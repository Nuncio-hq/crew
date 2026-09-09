use super::*;

fn candidate() -> RecapRuntimeContract {
    RecapRuntimeContract {
        command: Some("fixture-native"),
        selection: RecapSelectionContract::ExplicitModel,
    }
}

fn identity() -> RecapExecutableIdentity {
    RecapExecutableIdentity {
        resolved_path: PathBuf::from("/staging/bin/fixture-native"),
        version: "fixture-1".into(),
        fingerprint: "a".repeat(64),
        platform: "macos-aarch64".into(),
    }
}

fn selection() -> RecapSelection {
    RecapSelection {
        model: "configured-model".into(),
        profile: None,
        auth_available: true,
    }
}

#[test]
fn missing_executable_is_not_an_unsupported_installed_runtime() {
    assert_eq!(
        classify_recap("fixture", candidate(), None, &selection()).failure,
        RecapFailure::MissingExecutable
    );
}

#[test]
fn empty_or_auto_model_cannot_reach_an_adapter() {
    for model in ["", " ", "auto", "AUTO"] {
        let mut selected = selection();
        selected.model = model.into();
        assert_eq!(
            classify_recap("fixture", candidate(), Some(identity()), &selected).failure,
            RecapFailure::InvalidModelSelection,
            "model {model:?} must not silently use a runtime default"
        );
    }
}

#[test]
fn missing_staging_profile_and_auth_are_distinct() {
    let profile_candidate = RecapRuntimeContract {
        selection: RecapSelectionContract::StagingProfile,
        ..candidate()
    };
    assert_eq!(
        classify_recap("fixture", profile_candidate, Some(identity()), &selection()).failure,
        RecapFailure::MissingProfile
    );
    let mut selected = selection();
    selected.auth_available = false;
    assert_eq!(
        classify_recap("fixture", candidate(), Some(identity()), &selected).failure,
        RecapFailure::AuthRequired
    );
}

#[test]
fn discovery_success_never_certifies_generation() {
    assert_eq!(
        classify_recap("fixture", candidate(), Some(identity()), &selection()).failure,
        RecapFailure::UnverifiedCapability
    );
}

#[test]
fn unknown_version_never_matches_a_retained_positive_proof() {
    for version in ["", " ", "unknown", "UNKNOWN"] {
        let mut observed = identity();
        observed.version = version.into();
        assert!(!same_executable_proof(&observed, &observed));
        assert_eq!(
            classify_recap("fixture", candidate(), Some(observed), &selection()).failure,
            RecapFailure::UnknownExecutableVersion
        );
    }
}

#[test]
fn every_executable_identity_dimension_invalidates_retained_proof() {
    let original = identity();
    assert!(same_executable_proof(&original, &original));
    for changed in [
        RecapExecutableIdentity {
            version: "fixture-2".into(),
            ..original.clone()
        },
        RecapExecutableIdentity {
            fingerprint: "b".repeat(64),
            ..original.clone()
        },
        RecapExecutableIdentity {
            platform: "linux-x86_64".into(),
            ..original.clone()
        },
        RecapExecutableIdentity {
            resolved_path: PathBuf::from("/other/fixture-native"),
            ..original.clone()
        },
    ] {
        assert!(!same_executable_proof(&changed, &original));
    }
}

#[test]
fn malformed_identity_cannot_authorize_reuse() {
    for observed in [
        RecapExecutableIdentity {
            fingerprint: String::new(),
            ..identity()
        },
        RecapExecutableIdentity {
            platform: String::new(),
            ..identity()
        },
        RecapExecutableIdentity {
            resolved_path: PathBuf::from("relative/cli"),
            ..identity()
        },
    ] {
        assert!(!same_executable_proof(&observed, &observed));
        assert_eq!(
            classify_recap("fixture", candidate(), Some(observed), &selection()).failure,
            RecapFailure::InvalidExecutableIdentity
        );
    }
}

#[test]
fn relative_profile_is_not_a_staging_selection() {
    let mut selected = selection();
    selected.profile = Some(PathBuf::from("default"));
    assert_eq!(
        classify_recap(
            "fixture",
            RecapRuntimeContract {
                selection: RecapSelectionContract::StagingProfile,
                ..candidate()
            },
            Some(identity()),
            &selected
        )
        .failure,
        RecapFailure::MissingProfile
    );
}

#[test]
fn acp_only_runtime_has_no_one_shot_adapter() {
    assert_eq!(
        classify_recap(
            "fixture",
            RecapRuntimeContract {
                command: None,
                ..candidate()
            },
            Some(identity()),
            &selection()
        )
        .failure,
        RecapFailure::UnsupportedOneShot
    );
}

#[test]
fn option_shaped_model_is_rejected_at_admission() {
    let mut request = selection();
    request.model = "--dangerously-skip-permissions".into();
    assert_eq!(
        classify_recap("fixture", candidate(), Some(identity()), &request).failure,
        RecapFailure::InvalidModelSelection
    );
}
