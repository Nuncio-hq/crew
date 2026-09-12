use super::*;
use crate::managed_agents::recap_capability::{
    RecapAdmission, RecapExecutableIdentity, RecapSelection,
};

fn plan() -> RecapLaunchPlan {
    claude_recap_plan(
        Path::new("/staging/claude"),
        Path::new("/staging/recap-runs/fixture"),
        "claude-fable-5-1",
        b"synthetic recap input",
    )
    .unwrap()
}

fn final_output(model: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "type": "result", "subtype": "success", "is_error": false,
        "result": "Synthetic recap.", "modelUsage": {model: {"inputTokens": 12}}
    }))
    .unwrap()
}

#[test]
fn final_json_yields_only_the_recap_text() {
    assert_eq!(
        plan().parse_output(true, &final_output("claude-fable-5-1"), b""),
        Ok("Synthetic recap.".into())
    );
}

#[test]
fn nonzero_exit_and_secret_stderr_never_become_a_recap() {
    let failure = plan()
        .parse_output(
            false,
            &final_output("claude-fable-5-1"),
            b"SECRET_FIXTURE_TOKEN",
        )
        .unwrap_err();
    assert_eq!(failure, RecapRunFailure::NonzeroExit);
    assert!(!format!("{failure:?}").contains("SECRET_FIXTURE_TOKEN"));
}

#[test]
fn malformed_failed_or_missing_result_schema_is_rejected() {
    for output in [
        b"not-json".as_slice(),
        br#"{}"#,
        br#"{"type":"assistant","result":"text"}"#,
        br#"{"type":"result","subtype":"success","is_error":true,"result":"text"}"#,
        br#"{"type":"result","subtype":"success","is_error":false,"result":" "}"#,
    ] {
        assert_eq!(
            plan().parse_output(true, output, b""),
            Err(RecapRunFailure::InvalidOutput)
        );
    }
}

#[test]
fn missing_aliased_or_multiple_model_evidence_is_requested_only() {
    for output in [
        final_output("fable"),
        final_output("different-model"),
        br#"{"type":"result","subtype":"success","is_error":false,"result":"text"}"#.to_vec(),
        br#"{"type":"result","subtype":"success","is_error":false,"result":"text","modelUsage":{"claude-fable-5-1":{},"other":{}}}"#.to_vec(),
    ] {
        assert_eq!(plan().parse_output(true, &output, b""), Err(RecapRunFailure::ModelRequestedOnly));
    }
}

#[test]
fn either_output_stream_over_the_cap_is_rejected() {
    assert_eq!(
        plan().parse_output(true, &vec![b'x'; RECAP_OUTPUT_LIMIT + 1], b""),
        Err(RecapRunFailure::OutputLimit)
    );
    assert_eq!(
        plan().parse_output(
            true,
            &final_output("claude-fable-5-1"),
            &vec![b'x'; RECAP_OUTPUT_LIMIT + 1]
        ),
        Err(RecapRunFailure::OutputLimit)
    );
}

#[test]
fn oversized_input_and_implicit_selection_fail_before_command_creation() {
    assert_eq!(
        claude_recap_plan(
            Path::new("/staging/claude"),
            Path::new("/staging/run"),
            "model",
            &vec![b'x'; RECAP_INPUT_LIMIT + 1]
        )
        .unwrap_err(),
        RecapRunFailure::InputLimit
    );
    for model in ["", " ", "auto", "AUTO"] {
        assert_eq!(
            claude_recap_plan(
                Path::new("/staging/claude"),
                Path::new("/staging/run"),
                model,
                b"input"
            )
            .unwrap_err(),
            RecapRunFailure::InvalidSelection
        );
    }
}

#[cfg(unix)]
#[test]
fn native_command_denies_fake_builtin_mcp_hooks_and_plugins() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Stdio;
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("run");
    std::fs::create_dir(&root).unwrap();
    let tool_sentinel = fixture.path().join("tool-was-invoked");
    let audit = fixture.path().join("audit");
    let executable = fixture.path().join("fake-claude");
    let script = format!(
        r#"#!/bin/sh
tools=1; hooks=1; mcp=1; plugins=1; settings=1; persist=1
while [ "$#" -gt 0 ]; do
 case "$1" in
  --tools) shift; [ "$1" = '' ] && tools=0;;
  --safe-mode) hooks=0; plugins=0;;
  --strict-mcp-config) mcp=0;;
  --setting-sources) shift; [ "$1" = '' ] && settings=0;;
  --no-session-persistence) persist=0;;
 esac
 shift
done
if [ "$tools$hooks$mcp$plugins$settings$persist" != 000000 ]; then
 printf invoked > '{}'
fi
printf '%s\n' "$HOME" "$CLAUDE_CONFIG_DIR" "$TMPDIR" "$CLAUDE_CODE_SAFE_MODE" > '{}'
/bin/cat > '{}/stdin'
printf '%s' '{{"type":"result","subtype":"success","is_error":false,"result":"Synthetic recap.","modelUsage":{{"claude-fable-5-1":{{}}}}}}'
"#,
        tool_sentinel.display(),
        audit.display(),
        root.display()
    );
    std::fs::write(&executable, script).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let input_path = root.join("input");
    std::fs::write(&input_path, b"Synthetic fixture input.").unwrap();
    let plan = claude_recap_plan(
        &executable,
        &root,
        "claude-fable-5-1",
        b"Synthetic fixture input.",
    )
    .unwrap();
    let output = plan
        .command()
        .stdin(Stdio::from(std::fs::File::open(input_path).unwrap()))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        !tool_sentinel.exists(),
        "fixed native controls must disable builtin/MCP/hook/plugin invocation"
    );
    let audit = std::fs::read_to_string(audit).unwrap();
    assert_eq!(
        audit.lines().collect::<Vec<_>>(),
        vec![
            root.join("home").to_str().unwrap(),
            root.join("config").to_str().unwrap(),
            root.join("tmp").to_str().unwrap(),
            "1"
        ]
    );
    assert_eq!(
        std::fs::read(root.join("stdin")).unwrap(),
        b"Synthetic fixture input."
    );
    assert_eq!(
        plan.parse_output(output.status.success(), &output.stdout, &output.stderr)
            .unwrap(),
        "Synthetic recap."
    );
}

#[test]
fn model_cannot_be_an_argv_option() {
    let root = tempfile::tempdir().unwrap();
    for model in ["--dangerously-skip-permissions", "-m"] {
        assert!(matches!(
            claude_recap_plan(Path::new("/fake/claude"), root.path(), model, b"input"),
            Err(RecapRunFailure::InvalidSelection)
        ));
    }
}

#[test]
fn hermes_plan_copies_profile_and_requires_matching_usage_model() {
    let fixture = tempfile::tempdir().unwrap();
    let profile = fixture.path().join("source").join("profiles").join("scout");
    let root = fixture.path().join("run");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::write(profile.join("config.yaml"), "model: hermes-low\n").unwrap();
    let executable = fixture.path().join("hermes");
    std::fs::write(&executable, b"fixture").unwrap();

    let plan =
        hermes_recap_plan(&executable, &root, "hermes-low", &profile, b"thread input").unwrap();
    let usage = plan.usage_file.clone().unwrap();
    std::fs::write(&usage, r#"{"model":"hermes-low"}"#).unwrap();
    assert!(root.join("hermes/profiles/scout/config.yaml").is_file());
    let bound = bind_hermes_prompt(plan, b"thread input").unwrap();
    assert_eq!(bound.args.last().unwrap(), "thread input");
    assert_eq!(
        bound.parse_output(true, b"Hermes recap", b""),
        Ok("Hermes recap".into())
    );

    std::fs::write(&usage, r#"{"model":"other-model"}"#).unwrap();
    assert_eq!(
        bound.parse_output(true, b"Hermes recap", b""),
        Err(RecapRunFailure::ModelRequestedOnly)
    );
}

#[test]
fn hermes_profile_copy_rejects_symlinked_entries() {
    let fixture = tempfile::tempdir().unwrap();
    let profile = fixture.path().join("profiles").join("scout");
    let root = fixture.path().join("run");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(fixture.path().join("outside"), b"outside").unwrap();
    std::os::unix::fs::symlink(fixture.path().join("outside"), profile.join("link")).unwrap();
    let error = hermes_recap_plan(
        &fixture.path().join("hermes"),
        &root,
        "hermes-low",
        &profile,
        b"input",
    )
    .unwrap_err();
    assert_eq!(error, RecapRunFailure::ProfileUnavailable);
}

#[test]
fn hermes_plan_rechecks_profile_content_and_directory_identity() {
    let fixture = tempfile::tempdir().unwrap();
    let profile = fixture.path().join("profiles").join("scout");
    let root = fixture.path().join("run");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::write(profile.join("config.yaml"), "model: hermes-low\n").unwrap();
    let executable = fixture.path().join("hermes");
    std::fs::write(&executable, b"fixture").unwrap();

    let plan =
        hermes_recap_plan(&executable, &root, "hermes-low", &profile, b"thread input").unwrap();
    let admission = RecapAdmission {
        runtime_id: "hermes".into(),
        executable: RecapExecutableIdentity {
            resolved_path: executable,
            version: "fixture-1".into(),
            fingerprint: "a".repeat(64),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        },
        selection: RecapSelection {
            model: "hermes-low".into(),
            profile: Some(profile.clone()),
            profile_digest: plan.profile_digest.clone(),
            profile_identity: plan.profile_identity.clone(),
            auth_available: true,
        },
    };
    assert!(plan.profile_matches_admission(&admission));

    std::fs::write(profile.join("config.yaml"), "model: hermes-high\n").unwrap();
    assert!(!plan.profile_matches_admission(&admission));

    std::fs::remove_dir_all(&profile).unwrap();
    std::fs::create_dir_all(&profile).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::write(profile.join("config.yaml"), "model: hermes-low\n").unwrap();
    assert!(!plan.profile_matches_admission(&admission));
}

#[test]
fn hermes_profile_ref_accepts_only_home_or_named_profile_shape() {
    assert_eq!(
        hermes_profile_ref(Path::new("/staging/.hermes")),
        Some("default".to_string())
    );
    assert_eq!(
        hermes_profile_ref(Path::new("/staging/.hermes/profiles/scout")),
        Some("scout".to_string())
    );
    assert_eq!(hermes_profile_ref(Path::new("/staging/scout")), None);
    assert_eq!(
        hermes_profile_ref(Path::new("/staging/profiles/default")),
        None
    );
}
