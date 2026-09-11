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

fn known_contract() -> RecapRuntimeContract {
    super::super::known_acp_runtime_exact("claude")
        .expect("catalog contains claude")
        .recap_contract()
}

fn tool_probe() -> RecapToolProbeEvidence {
    RecapToolProbeEvidence {
        probe_id: "crew-recap-hostile-tool-v1".into(),
        tool_name: "context_engine".into(),
        request_observed: true,
        denied_before_effect: true,
        sentinel_before: "a".repeat(64),
        sentinel_after: "a".repeat(64),
    }
}

fn probe() -> RecapProbeAttestation {
    RecapProbeAttestation {
        runtime_id: "claude".into(),
        executable: RecapExecutableIdentity {
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            ..identity()
        },
        selection: RecapSelection {
            model: "claude-fable-5-1".into(),
            ..selection()
        },
        auth: RecapAuthBinding {
            service: "test-keyring-service".into(),
            reference: "test-auth-reference".into(),
        },
        one_shot_completed: true,
        tool_probe: tool_probe(),
        output: b"bounded recap output".to_vec(),
        effective_model: "claude-fable-5-1".into(),
        state_unchanged: true,
        process_reaped: true,
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

#[test]
fn runtime_ready_proof_requires_every_admission_guard() {
    let proof = RecapRuntimeReadyProof::for_test(
        "fixture",
        identity(),
        selection(),
        RecapGuarantees {
            one_shot: true,
            tool_isolation: true,
            state_isolation: true,
            process_containment: true,
        },
    );
    let admitted = admit_runtime_ready("fixture", candidate(), &proof, &selection())
        .expect("fully attested proof should admit");
    assert_eq!(admitted.runtime_id, "fixture");

    for field in [
        "one_shot",
        "tool_isolation",
        "state_isolation",
        "process_containment",
    ] {
        let mut guarantees = proof.guarantees_for_test();
        match field {
            "one_shot" => guarantees.one_shot = false,
            "tool_isolation" => guarantees.tool_isolation = false,
            "state_isolation" => guarantees.state_isolation = false,
            "process_containment" => guarantees.process_containment = false,
            _ => unreachable!(),
        }
        let proof =
            RecapRuntimeReadyProof::for_test("fixture", identity(), selection(), guarantees);
        let expected = match field {
            "one_shot" => RecapFailure::UnsupportedOneShot,
            "tool_isolation" => RecapFailure::UnsupportedToolIsolation,
            "state_isolation" => RecapFailure::UnsupportedStateIsolation,
            "process_containment" => RecapFailure::UnsupportedProcessContainment,
            _ => unreachable!(),
        };
        assert_eq!(
            admit_runtime_ready("fixture", candidate(), &proof, &selection()),
            Err(expected),
            "{field} must be explicitly attested"
        );
    }
}

#[test]
fn explicit_model_contract_rejects_profile_bearing_proof() {
    let mut selected = selection();
    selected.profile = Some(PathBuf::from("/staging/profile"));
    let proof = RecapRuntimeReadyProof::for_test(
        "fixture",
        identity(),
        selected.clone(),
        RecapGuarantees {
            one_shot: true,
            tool_isolation: true,
            state_isolation: true,
            process_containment: true,
        },
    );
    assert_eq!(
        admit_runtime_ready("fixture", candidate(), &proof, &selected),
        Err(RecapFailure::ProfileMismatch)
    );
}

#[test]
fn bounded_probe_is_the_only_path_to_a_positive_certification() {
    let certification = RecapRuntimeCertification::from_probe(known_contract(), probe())
        .expect("complete adapter evidence should certify");
    let parts = certification.parts();
    assert_eq!(parts.runtime_id, "claude");
    assert_eq!(parts.effective_model, parts.selection.model);
    assert_eq!(parts.output_digest.len(), 64);
    assert_eq!(parts.tool_probe_digest.len(), 64);
    assert!(parts.guarantees.one_shot);
    assert!(parts.guarantees.tool_isolation);
    assert!(parts.guarantees.state_isolation);
    assert!(parts.guarantees.process_containment);
}

#[test]
fn certification_requires_catalog_identity_and_all_probe_evidence() {
    let mut invalid = probe();
    invalid.runtime_id = "unknown-runtime".into();
    assert_eq!(
        RecapRuntimeCertification::from_probe(known_contract(), invalid),
        Err(RecapFailure::RuntimeMismatch)
    );

    let mut invalid = probe();
    invalid.one_shot_completed = false;
    assert_eq!(
        RecapRuntimeCertification::from_probe(known_contract(), invalid),
        Err(RecapFailure::UnsupportedOneShot)
    );

    let mut invalid = probe();
    invalid.tool_probe.request_observed = false;
    assert_eq!(
        RecapRuntimeCertification::from_probe(known_contract(), invalid),
        Err(RecapFailure::InvalidToolProbeEvidence)
    );

    let mut invalid = probe();
    invalid.tool_probe.sentinel_after = "b".repeat(64);
    assert_eq!(
        RecapRuntimeCertification::from_probe(known_contract(), invalid),
        Err(RecapFailure::InvalidToolProbeEvidence)
    );

    let mut invalid = probe();
    invalid.effective_model = "other-model".into();
    assert_eq!(
        RecapRuntimeCertification::from_probe(known_contract(), invalid),
        Err(RecapFailure::EffectiveModelMismatch)
    );

    let mut invalid = probe();
    invalid.output = Vec::new();
    assert_eq!(
        RecapRuntimeCertification::from_probe(known_contract(), invalid),
        Err(RecapFailure::EmptyProbeOutput)
    );

    let mut invalid = probe();
    invalid.state_unchanged = false;
    assert_eq!(
        RecapRuntimeCertification::from_probe(known_contract(), invalid),
        Err(RecapFailure::ProbeStateChanged)
    );

    let mut invalid = probe();
    invalid.process_reaped = false;
    assert_eq!(
        RecapRuntimeCertification::from_probe(known_contract(), invalid),
        Err(RecapFailure::ProbeProcessNotReaped)
    );

    let mut invalid = probe();
    invalid.auth.service = "".into();
    assert_eq!(
        RecapRuntimeCertification::from_probe(known_contract(), invalid),
        Err(RecapFailure::InvalidAuthBinding)
    );
}
