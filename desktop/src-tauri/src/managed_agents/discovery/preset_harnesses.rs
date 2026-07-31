use std::path::PathBuf;
use std::sync::OnceLock;

use crate::managed_agents::{
    AcpAvailabilityStatus, AcpRuntimeCatalogEntry, AuthStatus, HarnessSource,
};

use super::{normalize_agent_args, normalize_command_identity};

const CURSOR_AVATAR_URL: &str = "https://cursor.com/marketing-static/icon-192x192-light.png";
const OMP_AVATAR_URL: &str = "https://raw.githubusercontent.com/can1357/oh-my-pi/667111575ebba136dadfd6989379e7f67e0d40d9/assets/icon.svg";
const GROK_AVATAR_URL: &str = "https://grok.com/images/android-chrome-192x192.png";
const OPENCODE_AVATAR_URL: &str = "https://opencode.ai/apple-touch-icon-v3.png";
const KIMI_AVATAR_URL: &str = "https://raw.githubusercontent.com/MoonshotAI/kimi-cli/4a550effdfcb29a25a5d325bf935296cc50cd417/web/public/logo.png";
const AMP_AVATAR_URL: &str = "https://ampcode.com/app-icon.png?v=3";
const HERMES_AVATAR_URL: &str = "https://raw.githubusercontent.com/NousResearch/hermes-agent/6ad632b/website/static/img/logo.png";
const OPENCLAW_AVATAR_URL: &str = "https://docs.openclaw.ai/assets/openclaw.svg";
const DEVIN_AVATAR_URL: &str = "https://docs.devin.ai/mintlify-assets/_mintlify/favicons/cognitionai/Ycul7J1XWDV1FX48/_generated/favicon/android-chrome-192x192.png";

#[derive(Clone, Copy)]
pub(super) struct PresetHarness {
    pub(super) id: &'static str,
    pub(super) label: &'static str,
    pub(super) command: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) avatar_url: &'static str,
    pub(super) install_instructions_url: &'static str,
    pub(super) install_hint: &'static str,
    pub(super) underlying_cli: Option<&'static str>,
}

pub(super) fn preset_catalog_entry(
    def: &PresetHarness,
    resolve: impl Fn(&str) -> Option<PathBuf>,
) -> AcpRuntimeCatalogEntry {
    let resolved = resolve(def.command);
    let underlying_cli_path = def
        .underlying_cli
        .and_then(&resolve)
        .map(|path| path.display().to_string());
    let availability = if resolved.is_some() {
        AcpAvailabilityStatus::Available
    } else if underlying_cli_path.is_some() {
        AcpAvailabilityStatus::AdapterMissing
    } else {
        AcpAvailabilityStatus::NotInstalled
    };
    let binary_path = resolved.as_ref().map(|path| path.display().to_string());
    let command = resolved.map(|_| def.command.to_string());
    AcpRuntimeCatalogEntry {
        id: def.id.to_string(),
        label: def.label.to_string(),
        avatar_url: def.avatar_url.to_string(),
        availability,
        command,
        binary_path,
        default_args: normalize_agent_args(
            def.command,
            def.args.iter().map(|arg| arg.to_string()).collect(),
        ),
        mcp_command: None,
        model_env_var: None,
        provider_env_var: None,
        thinking_env_var: None,
        install_hint: def.install_hint.to_string(),
        install_instructions_url: def.install_instructions_url.to_string(),
        can_auto_install: false,
        requires_external_cli: false,
        underlying_cli_path,
        node_required: false,
        auth_status: AuthStatus::NotApplicable,
        login_hint: None,
        source: HarnessSource::Preset,
        definition_env: Default::default(),
    }
}

pub(super) fn preset_avatar_url(command: &str) -> Option<String> {
    let identity = normalize_command_identity(command);
    PRESET_HARNESSES
        .iter()
        .find(|preset| normalize_command_identity(preset.command) == identity)
        .map(|preset| preset.avatar_url.to_string())
}

pub(crate) fn preset_harness_definitions(
) -> Vec<crate::managed_agents::custom_harnesses::HarnessDefinition> {
    PRESET_HARNESSES
        .iter()
        .map(
            |preset| crate::managed_agents::custom_harnesses::HarnessDefinition {
                id: preset.id.to_string(),
                label: preset.label.to_string(),
                command: preset.command.to_string(),
                args: preset.args.iter().map(|arg| arg.to_string()).collect(),
                env: Default::default(),
                install_instructions_url: preset.install_instructions_url.to_string(),
                install_hint: preset.install_hint.to_string(),
            },
        )
        .collect()
}

pub(crate) fn preset_harness_ids() -> &'static [&'static str] {
    static IDS: OnceLock<Vec<&'static str>> = OnceLock::new();
    IDS.get_or_init(|| PRESET_HARNESSES.iter().map(|preset| preset.id).collect())
        .as_slice()
}

pub(super) const PRESET_HARNESSES: &[PresetHarness] = &[
    preset("cursor", "Cursor", "cursor-agent", &["acp"], CURSOR_AVATAR_URL, "https://cursor.com/downloads", "Buzz talks to Cursor through the cursor-agent CLI's ACP mode."),
    preset("omp", "Oh My Pi", "omp", &["acp"], OMP_AVATAR_URL, "https://github.com/can1357/oh-my-pi", "Buzz talks to Oh My Pi through its CLI's ACP mode (omp acp)."),
    preset("grok", "Grok Build", "grok", &["agent", "--always-approve", "stdio"], GROK_AVATAR_URL, "https://build.x.ai/docs", "Buzz talks to Grok Build through its CLI's agent stdio mode."),
    preset("opencode", "OpenCode", "opencode", &["acp"], OPENCODE_AVATAR_URL, "https://opencode.ai/docs", "Buzz talks to OpenCode through its CLI's ACP mode (opencode acp)."),
    preset("kimi", "Kimi Code", "kimi", &["acp"], KIMI_AVATAR_URL, "https://kimi.ai/download", "Buzz talks to Kimi Code through its CLI's ACP mode (kimi acp)."),
    PresetHarness {
        id: "amp",
        label: "Amp",
        command: "amp-acp",
        args: &[],
        avatar_url: AMP_AVATAR_URL,
        install_instructions_url: "https://github.com/tao12345666333/amp-acp",
        install_hint: "Buzz talks to the Amp CLI through the amp-acp adapter. Follow the setup guide to install the adapter so the amp-acp command is on your PATH.",
        underlying_cli: Some("amp"),
    },
    preset("hermes", "Hermes Agent", "hermes-acp", &[], HERMES_AVATAR_URL, "https://hermes-agent.nousresearch.com", "Buzz talks to Hermes Agent through its hermes-acp command."),
    preset(
        "openclaw",
        "OpenClaw",
        "openclaw",
        &["acp"],
        OPENCLAW_AVATAR_URL,
        "https://docs.openclaw.ai/start/getting-started",
        "Buzz talks to OpenClaw through its ACP mode (openclaw acp), which relies on the OpenClaw Gateway daemon. Follow the setup guide to install both.\n\n⚠️  Execution-locus note: `openclaw acp` runs tools inside the OpenClaw Gateway daemon, not in the Desktop process. Desktop-injected BUZZ_* env vars are visible to the `openclaw` harness process itself, but do NOT automatically reach the Gateway's execution environment. If your tools or agent logic needs BUZZ_* credentials at execution time, set them on the Gateway's own environment separately.",
    ),
    preset("devin", "Devin", "devin", &["acp"], DEVIN_AVATAR_URL, "https://docs.devin.ai/get-started/devin-intro", "Buzz talks to Devin for Terminal through its documented ACP stdio mode (devin acp). Install it with: curl -fsSL https://cli.devin.ai/install.sh | bash."),
];

const fn preset(
    id: &'static str,
    label: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    avatar_url: &'static str,
    install_instructions_url: &'static str,
    install_hint: &'static str,
) -> PresetHarness {
    PresetHarness {
        id,
        label,
        command,
        args,
        avatar_url,
        install_instructions_url,
        install_hint,
        underlying_cli: None,
    }
}
