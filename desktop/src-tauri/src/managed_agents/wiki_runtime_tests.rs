use super::*;
use std::collections::BTreeMap;

fn hermes(profile: &str) -> WikiRuntimeSelection {
    WikiRuntimeSelection {
        runtime_id: "hermes".into(),
        model: None,
        profile: Some(profile.into()),
    }
}

fn claude(model: &str) -> WikiRuntimeSelection {
    WikiRuntimeSelection {
        runtime_id: "claude".into(),
        model: Some(model.into()),
        profile: None,
    }
}

fn codex(model: &str) -> WikiRuntimeSelection {
    WikiRuntimeSelection {
        runtime_id: "codex".into(),
        model: Some(model.into()),
        profile: None,
    }
}

fn page_snapshot(content: &str) -> (PlannedPage, RepoSnapshot) {
    (
        PlannedPage {
            slug: "overview".into(),
            title: "Overview".into(),
            section: "overview".into(),
            source_files: vec!["src/lib.rs".into()],
        },
        RepoSnapshot {
            commit: "deadbeef".into(),
            branch: "main".into(),
            source_revision: "git:deadbeef".into(),
            files: vec!["src/lib.rs".into()],
            contents: BTreeMap::from([("src/lib.rs".into(), content.into())]),
            ..RepoSnapshot::default()
        },
    )
}

#[test]
fn selection_is_independent_from_employee_agent_settings() {
    assert!(hermes("wiki-proof").validate().is_ok());
    assert!(claude("claude-fable-5-1").validate().is_ok());
    assert_eq!(hermes("wiki-proof").model, None);
}

#[test]
fn unsupported_and_missing_runtime_selections_fail_closed() {
    let unsupported = WikiRuntimeSelection {
        runtime_id: "heuristic".into(),
        model: None,
        profile: None,
    };
    assert_eq!(
        unsupported.validate(),
        Err(WikiRuntimeFailure::UnsupportedRuntime("heuristic".into()))
    );
    assert_eq!(
        WikiRuntimeSelection {
            runtime_id: "hermes".into(),
            model: None,
            profile: None,
        }
        .validate(),
        Err(WikiRuntimeFailure::MissingProfile)
    );
    assert!(matches!(
        WikiRuntimeSelection {
            runtime_id: " hermes".into(),
            model: None,
            profile: Some("wiki-proof".into()),
        }
        .validate(),
        Err(WikiRuntimeFailure::UnsupportedRuntime(_))
    ));
}

#[test]
fn profile_owned_runtime_rejects_model_override() {
    assert_eq!(
        WikiRuntimeSelection {
            runtime_id: "hermes".into(),
            model: Some("ignored-model".into()),
            profile: Some("wiki-proof".into()),
        }
        .validate(),
        Err(WikiRuntimeFailure::ProfileOwnsModel)
    );
}

#[test]
fn codex_profile_is_rejected_without_a_staged_config_layer() {
    assert_eq!(
        WikiRuntimeSelection {
            runtime_id: "codex".into(),
            model: Some("codex-fable-5-1".into()),
            profile: Some("wiki-proof".into()),
        }
        .validate(),
        Err(WikiRuntimeFailure::UnsupportedProfile)
    );
}

#[test]
fn prompt_contains_immutable_source_contents_and_never_filename_only_context() {
    let (page, snapshot) = page_snapshot("pub fn canonical() -> &'static str { \"fixture\" }\n");
    let prompt = build_prompt(&page, &snapshot, "en").expect("prompt");
    assert!(prompt.contains("pub fn canonical()"));
    assert!(prompt.contains("git:deadbeef"));
    assert!(prompt.contains("src/lib.rs"));
}

#[test]
fn prompt_is_rejected_when_complete_source_context_exceeds_bound() {
    let (page, snapshot) = page_snapshot(&"x".repeat(WIKI_RUNTIME_INPUT_LIMIT));
    assert_eq!(
        build_prompt(&page, &snapshot, "en"),
        Err(WikiRuntimeFailure::InputLimit)
    );
}

#[test]
fn generated_links_are_limited_to_existing_source_ranges() {
    let (page, snapshot) = page_snapshot("first\nsecond\n");
    assert!(validate_generated_links(
        &page,
        &snapshot,
        "[source](buzz://file?path=src/lib.rs&lines=1-2)"
    )
    .is_ok());
    for markdown in [
        "[external](https://example.com)",
        "[other](buzz://file?path=src/other.rs&lines=1-1)",
        "[bad-range](buzz://file?path=src/lib.rs&lines=0-3)",
    ] {
        assert_eq!(
            validate_generated_links(&page, &snapshot, markdown),
            Err(WikiRuntimeFailure::InvalidOutput)
        );
    }
}

#[cfg(unix)]
fn fake_runtime(script: &str) -> (tempfile::TempDir, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("fixture dir");
    let executable = dir.path().join("fake-runtime");
    std::fs::write(&executable, script).expect("script");
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
        .expect("permissions");
    (dir, executable)
}

#[cfg(unix)]
struct HermesHomeGuard(Option<std::ffi::OsString>);

#[cfg(unix)]
impl HermesHomeGuard {
    fn set(path: &Path) -> Self {
        let previous = std::env::var_os("HERMES_HOME");
        std::env::set_var("HERMES_HOME", path);
        Self(previous)
    }
}

#[cfg(unix)]
impl Drop for HermesHomeGuard {
    fn drop(&mut self) {
        match self.0.take() {
            Some(previous) => std::env::set_var("HERMES_HOME", previous),
            None => std::env::remove_var("HERMES_HOME"),
        }
    }
}

#[cfg(unix)]
#[test]
fn profile_root_symlink_is_rejected_before_canonicalization() {
    let fixture = tempfile::tempdir().expect("fixture dir");
    let real = fixture.path().join("real");
    let link = fixture.path().join("profile");
    std::fs::create_dir(&real).expect("real profile");
    std::os::unix::fs::symlink(&real, &link).expect("profile symlink");

    assert_eq!(
        canonical_profile_source(&link),
        Err(WikiRuntimeFailure::ProfileUnavailable)
    );
}

#[cfg(unix)]
#[test]
fn fake_runtime_positive_path_uses_source_and_isolated_environment() {
    let (fixture, executable) =
            fake_runtime(
                "#!/bin/sh\nseen=no\nfor arg in \"$@\"; do\n  [ \"$arg\" = \"--toolsets\" ] && seen=yes\ndone\nprintf '%s|%s|%s|%s' \"$HOME\" \"$HERMES_HOME\" \"$1\" \"$seen\"\n",
            );
    let state = fixture.path().join("state");
    let generator =
        WikiRuntimeGenerator::with_executable(hermes("wiki-proof"), executable, state.clone())
            .expect("generator");
    let (page, snapshot) = page_snapshot("source-secret-fixture");
    let output = generator
        .generate(&page, &snapshot, "en")
        .expect("generated");
    assert!(output.contains(state.join("home").to_str().expect("home")));
    assert!(output.contains(state.join("hermes").to_str().expect("hermes")));
    assert!(output.contains("|yes"));
    assert!(
        !output.contains("source-secret-fixture"),
        "fake output is not the model; prompt is argv and not echoed"
    );
}

#[cfg(unix)]
#[test]
fn hermes_profile_launch_keeps_staged_profile_config_enabled() {
    let (fixture, executable) = fake_runtime("#!/bin/sh\nprintf '%s' \"$@\"\n");
    let state = fixture.path().join("state");
    let generator = WikiRuntimeGenerator::with_executable(hermes("wiki-proof"), executable, state)
        .expect("generator");
    let args = generator
        .command(Some("prompt"))
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(args.windows(2).any(|pair| pair == ["-p", "wiki-proof"]));
    assert!(args.iter().any(|arg| arg == "--toolsets"));
    assert!(args.iter().any(|arg| arg == "--safe-mode"));
    assert!(!args.iter().any(|arg| arg == "--ignore-user-config"));
}

#[cfg(unix)]
#[test]
fn hermes_generation_stages_valid_profile_and_reasserts_native_safe_mode() {
    let (fixture, executable) = fake_runtime(
        r#"#!/bin/sh
set -eu
profile="$HERMES_HOME/profiles/wiki-proof"
grep -q 'provider: profile-provider' "$profile/config.yaml"
grep -q 'default: profile-model' "$profile/config.yaml"
[ -d "$HERMES_MANAGED_DIR" ]
[ -z "$(ls -A "$HERMES_MANAGED_DIR")" ]
safe="${HERMES_SAFE_MODE:-}"
ignore_rules="${HERMES_IGNORE_RULES:-}"
ignore_user_config="${HERMES_IGNORE_USER_CONFIG:-}"
managed_dir=""
apply_env_file() {
  file="$1"
  [ -f "$file" ] || return 0
  while IFS='=' read -r key value; do
    case "$key" in
      HERMES_SAFE_MODE) safe="$value" ;;
      HERMES_IGNORE_RULES) ignore_rules="$value" ;;
      HERMES_IGNORE_USER_CONFIG) ignore_user_config="$value" ;;
      HERMES_MANAGED_DIR) managed_dir="$value" ;;
    esac
  done < "$file"
}
apply_env_file "$profile/.env"
if [ "$managed_dir" = "managed" ]; then
  apply_env_file "$PWD/managed/.env"
fi
for arg in "$@"; do
  if [ "$arg" = "--safe-mode" ]; then
    safe=1
    ignore_rules=1
    ignore_user_config=1
  fi
done
if [ "$safe" != "1" ] || [ "$ignore_rules" != "1" ] || [ "$ignore_user_config" != "1" ]; then
  printf 'child-ran' > "$PWD/child-ran"
  exit 91
fi
provider=$(sed -n 's/^  provider: //p' "$profile/config.yaml")
model=$(sed -n 's/^  default: //p' "$profile/config.yaml")
printf 'safe-mode-page|%s|%s' "$provider" "$model"
"#,
    );
    let source_home = fixture.path().join("source-hermes");
    let source_profile = source_home.join("profiles/wiki-proof");
    std::fs::create_dir_all(&source_profile).expect("source profile");
    std::fs::write(
            source_profile.join("config.yaml"),
            "model:\n  provider: profile-provider\n  default: profile-model\nhooks:\n  pre_tool_call: hook-marker\nplugins:\n  enabled: [plugin-marker]\nmcp_servers:\n  marker: mcp-marker\n",
        )
        .expect("valid profile config");
    let profile_env = b"PROFILE_NOTE=preserve-byte-for-byte\n";
    std::fs::write(source_profile.join(".env"), profile_env).expect("profile dotenv");
    let profile_op_env = b"OP_NOTE=preserve-op-byte-for-byte\r\n";
    std::fs::write(source_profile.join(".op.env"), profile_op_env).expect("profile op dotenv");
    let state = fixture.path().join("state");
    let _path_guard = crate::managed_agents::lock_path_mutex();
    let _home_guard = HermesHomeGuard::set(&source_home);

    let generator =
        WikiRuntimeGenerator::with_executable(hermes("wiki-proof"), executable, state.clone())
            .expect("generator");
    generator
        .stage_hermes_profile()
        .expect("profile staging and config validation");
    assert_eq!(
        std::fs::read(state.join("hermes/profiles/wiki-proof/.env")).expect("staged dotenv"),
        profile_env
    );
    assert_eq!(
        std::fs::read(state.join("hermes/profiles/wiki-proof/.op.env")).expect("staged op dotenv"),
        profile_op_env
    );
    assert!(state.join("managed").is_dir());
    assert!(std::fs::read_dir(state.join("managed"))
        .expect("private managed directory")
        .next()
        .is_none());
    let (page, snapshot) = page_snapshot("source-secret-fixture");
    let output = generator
        .generate(&page, &snapshot, "en")
        .expect("safe-mode generation");
    assert_eq!(output, "safe-mode-page|profile-provider|profile-model");
    assert!(!state.join("child-ran").exists());
}

#[test]
fn hermes_dotenv_routing_assignments_are_rejected_without_rewriting_safe_bytes() {
    for line in [
        "HERMES_HOME=/tmp/redirect",
        "  export HERMES_MANAGED_DIR = /tmp/redirect",
        "'HERMES_HOME'='/tmp/redirect'",
        "export 'HERMES_MANAGED_DIR' = '/tmp/redirect'",
        "HERMES_HOME",
        "HERMES_MANAGED_DIR+=/tmp/redirect",
        "HERMES_HOME\nHERMES_MANAGED_DIR=/tmp/redirect",
    ] {
        assert!(
            dotenv_line_has_hermes_binding_assignment(line),
            "routing line should be rejected: {line:?}"
        );
    }
    for line in [
        "# HERMES_HOME=/tmp/comment",
        "PROFILE_HERMES_HOME=/tmp/other",
        "PROFILE_NOTE='HERMES_HOME=/tmp/value'",
        "export PROFILE_NOTE=preserve",
    ] {
        assert!(
            !dotenv_line_has_hermes_binding_assignment(line),
            "safe line should remain untouched: {line:?}"
        );
    }
    let safe = b"PROFILE_NOTE=preserve\r\n";
    assert!(validate_hermes_dotenv_bytes(safe).is_ok());
    assert_eq!(safe, b"PROFILE_NOTE=preserve\r\n");
    for contents in [
        "\u{00a0}HERMES_HOME=/tmp/redirect",
        "export\u{00a0}HERMES_MANAGED_DIR=/tmp/redirect",
        "NOTE=x\rHERMES_HOME=/tmp/redirect",
    ] {
        assert_eq!(
            validate_hermes_dotenv_bytes(contents.as_bytes()),
            Err(WikiRuntimeFailure::ProfileBinding),
            "routing form should be rejected: {contents:?}"
        );
    }
    assert_eq!(
        validate_hermes_dotenv_bytes(b"\xef\xbb\xbfPROFILE_NOTE=value"),
        Err(WikiRuntimeFailure::ProfileBinding)
    );
    assert_eq!(
        validate_hermes_dotenv_bytes(b"PROFILE_NOTE=value\0hidden"),
        Err(WikiRuntimeFailure::ProfileBinding)
    );
    assert_eq!(
        validate_hermes_dotenv_bytes(b"\xff"),
        Err(WikiRuntimeFailure::ProfileBinding)
    );
}

#[test]
fn hermes_secret_gate_rejects_enabled_yaml_forms_and_merges() {
    for enabled in ["true", "yes", "on"] {
        let config = serde_yaml::from_str::<serde_yaml::Value>(&format!(
            "secrets:\n  onepassword:\n    enabled: {enabled}\n"
        ))
        .expect("secret config");
        assert_eq!(
            reject_enabled_hermes_secret_sources(&config),
            Err(WikiRuntimeFailure::ProfileBinding),
            "enabled form should be rejected: {enabled}"
        );
    }
    for config_text in [
        "secrets:\n  onepassword:\n    enabled: false\n",
        "secrets:\n  onepassword:\n    enabled: null\n",
        "secrets: {}\n",
        "{}\n",
    ] {
        let config =
            serde_yaml::from_str::<serde_yaml::Value>(config_text).expect("disabled secret config");
        assert!(reject_enabled_hermes_secret_sources(&config).is_ok());
    }
    let merged = serde_yaml::from_str::<serde_yaml::Value>(
        "defaults: &defaults\n  enabled: true\nsecrets:\n  onepassword:\n    <<: *defaults\n",
    )
    .expect("merged secret config");
    assert_eq!(
        reject_enabled_hermes_secret_sources(&merged),
        Err(WikiRuntimeFailure::ProfileBinding)
    );
    let tagged = serde_yaml::from_str::<serde_yaml::Value>(
        "secrets:\n  onepassword:\n    enabled: !unsafe true\n",
    )
    .expect("tagged secret config");
    assert_eq!(
        reject_enabled_hermes_secret_sources(&tagged),
        Err(WikiRuntimeFailure::ProfileBinding)
    );
}

#[cfg(unix)]
#[test]
fn staging_rejects_routing_assignments_in_each_profile_dotenv_file() {
    for (file_name, contents) in [
        (".env", "\u{00a0}HERMES_HOME=/tmp/redirect\n"),
        (
            ".op.env",
            "export\u{00a0}HERMES_MANAGED_DIR=/tmp/redirect\n",
        ),
        (".env", "NOTE=x\rHERMES_HOME=/tmp/redirect\n"),
    ] {
        let fixture = tempfile::tempdir().expect("fixture dir");
        let source_home = fixture.path().join("source-hermes");
        let source_profile = source_home.join("profiles/wiki-proof");
        std::fs::create_dir_all(&source_profile).expect("profile");
        std::fs::write(
            source_profile.join("config.yaml"),
            "model:\n  provider: profile-provider\n  default: profile-model\n",
        )
        .expect("profile config");
        std::fs::write(source_profile.join(file_name), contents).expect("profile dotenv");
        let state = fixture.path().join("state");
        let executable = fixture.path().join("fake-runtime");
        std::fs::write(
            &executable,
            "#!/bin/sh\nprintf 'child-ran' > \"$PWD/child-ran\"\nexit 0\n",
        )
        .expect("fake runtime");
        std::fs::set_permissions(
            &executable,
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .expect("fake permissions");
        let _path_guard = crate::managed_agents::lock_path_mutex();
        let _home_guard = HermesHomeGuard::set(&source_home);
        let generator =
            WikiRuntimeGenerator::with_executable(hermes("wiki-proof"), executable, state.clone())
                .expect("generator");

        assert_eq!(
            generator.stage_hermes_profile(),
            Err(WikiRuntimeFailure::ProfileBinding),
            "routing file should be rejected: {file_name}"
        );
        assert!(!state.join("child-ran").exists());
    }
}

#[cfg(unix)]
#[test]
fn missing_profile_dotenv_is_created_and_empty() {
    let fixture = tempfile::tempdir().expect("fixture dir");
    let source_home = fixture.path().join("source-hermes");
    let source_profile = source_home.join("profiles/wiki-proof");
    std::fs::create_dir_all(&source_profile).expect("profile");
    std::fs::write(source_profile.join(".op.env"), b"OP_NOTE=preserve\n").expect("op dotenv");
    let state = fixture.path().join("state");
    let _path_guard = crate::managed_agents::lock_path_mutex();
    let _home_guard = HermesHomeGuard::set(&source_home);
    let generator = WikiRuntimeGenerator::with_executable(
        hermes("wiki-proof"),
        fixture.path().join("fake-runtime"),
        state.clone(),
    )
    .expect("generator");

    generator.stage_hermes_profile().expect("dotenv validation");

    assert_eq!(
        std::fs::read(state.join("hermes/profiles/wiki-proof/.env")).expect("created dotenv"),
        b""
    );
    assert_eq!(
        std::fs::read(state.join("hermes/profiles/wiki-proof/.op.env")).expect("op dotenv"),
        b"OP_NOTE=preserve\n"
    );
}

#[cfg(unix)]
#[test]
fn enabled_hermes_external_secret_source_is_rejected_before_child_launch() {
    let (fixture, executable) =
        fake_runtime("#!/bin/sh\nprintf 'child-ran' > \"$PWD/child-ran\"\nexit 0\n");
    let source_home = fixture.path().join("source-hermes");
    let source_profile = source_home.join("profiles/wiki-proof");
    std::fs::create_dir_all(&source_profile).expect("source profile");
    std::fs::write(
            source_profile.join("config.yaml"),
            "model:\n  provider: profile-provider\n  default: profile-model\nsecrets:\n  onepassword:\n    enabled: true\n",
        )
        .expect("profile config");
    std::fs::write(source_profile.join(".env"), b"PROFILE_NOTE=preserve\n")
        .expect("profile dotenv");
    let state = fixture.path().join("state");
    let _path_guard = crate::managed_agents::lock_path_mutex();
    let _home_guard = HermesHomeGuard::set(&source_home);
    let generator =
        WikiRuntimeGenerator::with_executable(hermes("wiki-proof"), executable, state.clone())
            .expect("generator");

    assert_eq!(
        generator.stage_hermes_profile(),
        Err(WikiRuntimeFailure::ProfileBinding)
    );
    assert!(!state.join("child-ran").exists());
}

#[cfg(unix)]
#[test]
fn malformed_staged_hermes_config_fails_before_child_launch() {
    let (fixture, executable) =
        fake_runtime("#!/bin/sh\nprintf 'child-ran' > \"$PWD/child-ran\"\nexit 0\n");
    let source_home = fixture.path().join("source-hermes");
    let source_profile = source_home.join("profiles/wiki-proof");
    std::fs::create_dir_all(&source_profile).expect("source profile");
    std::fs::write(source_profile.join("config.yaml"), "model: [").expect("broken config");
    let state = fixture.path().join("state");
    let _path_guard = crate::managed_agents::lock_path_mutex();
    let _home_guard = HermesHomeGuard::set(&source_home);
    let generator =
        WikiRuntimeGenerator::with_executable(hermes("wiki-proof"), executable, state.clone())
            .expect("generator");
    let error = generator
        .stage_hermes_profile()
        .expect_err("malformed config must fail before launch");
    assert_eq!(error, WikiRuntimeFailure::InvalidProfileConfig);
    assert_eq!(
        error.to_string(),
        "The selected Wiki runtime profile config is invalid."
    );
    assert!(!state.join("child-ran").exists());
}

#[test]
fn staged_hermes_config_allows_missing_and_empty_first_run_states() {
    let fixture = tempfile::tempdir().expect("fixture dir");
    let profile = fixture.path().join("profile");
    std::fs::create_dir_all(&profile).expect("profile");
    assert!(validate_staged_hermes_profile_config(&profile).is_ok());
    std::fs::write(profile.join("config.yaml"), "").expect("empty config");
    assert!(validate_staged_hermes_profile_config(&profile).is_ok());
}

#[cfg(unix)]
#[test]
fn nonzero_runtime_is_not_synthesized_as_success() {
    let (_fixture, executable) = fake_runtime("#!/bin/sh\nprintf 'provider failure' >&2\nexit 7\n");
    let generator = WikiRuntimeGenerator::with_executable(
        claude("claude-fable-5-1"),
        executable,
        tempfile::tempdir().expect("state").keep(),
    )
    .expect("generator");
    let (page, snapshot) = page_snapshot("source");
    let error = generator
        .generate(&page, &snapshot, "en")
        .expect_err("nonzero runtime must fail");
    assert!(matches!(
        error,
        WikiError::Generate(message)
            if message.contains(&WikiRuntimeFailure::NonzeroExit.to_string())
                && message.contains("runtime=claude")
                && message.contains("model=claude-fable-5-1")
    ));
}

#[cfg(unix)]
#[test]
fn codex_style_runtime_receives_the_complete_prompt_on_stdin() {
    let (_fixture, executable) =
        fake_runtime("#!/bin/sh\nread -r first || exit 9\nprintf 'stdin-ok'");
    let generator = WikiRuntimeGenerator::with_executable(
        codex("codex-fable-5-1"),
        executable,
        tempfile::tempdir().expect("state").keep(),
    )
    .expect("generator");
    let (page, snapshot) = page_snapshot("immutable-source");
    let output = generator
        .generate(&page, &snapshot, "en")
        .expect("generated from stdin");
    assert_eq!(output, "stdin-ok");
}
