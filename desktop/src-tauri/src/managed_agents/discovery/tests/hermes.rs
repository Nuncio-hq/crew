use super::normalize_agent_args;

#[test]
fn known_acp_runtime_resolves_hermes_identities() {
    for command in [
        "hermes",
        "hermes-acp",
        "hermes-agent",
        "/opt/hermes/bin/hermes-acp",
        r"C:\Users\test\AppData\Roaming\npm\hermes-acp.cmd",
    ] {
        let runtime = super::super::known_acp_runtime(command)
            .unwrap_or_else(|| panic!("expected hermes runtime for {command}"));
        assert_eq!(runtime.id, "hermes");
        assert_eq!(runtime.label, "Hermes Agent");
        assert_eq!(runtime.mcp_command, Some("buzz-dev-mcp"));
        assert!(runtime.model_env_var.is_none());
        assert!(runtime.provider_env_var.is_none());
        assert!(runtime.provider_locked);
        assert_eq!(
            runtime.default_env,
            &[("HERMES_ACP_SKIP_CONFIGURED_MCP", "1")]
        );
        assert!(runtime.auth_probe_args.is_none());
        assert!(runtime.supports_acp_model_switching);
    }
}

#[test]
fn normalizes_hermes_default_agent_args() {
    assert_eq!(
        normalize_agent_args("hermes", Vec::new()),
        vec!["acp".to_string()]
    );
    assert_eq!(
        normalize_agent_args("hermes-acp", Vec::new()),
        Vec::<String>::new()
    );
    assert_eq!(
        normalize_agent_args("hermes-agent", Vec::new()),
        Vec::<String>::new()
    );
    assert_eq!(
        normalize_agent_args("hermes-acp", vec!["acp".into()]),
        Vec::<String>::new()
    );
}
