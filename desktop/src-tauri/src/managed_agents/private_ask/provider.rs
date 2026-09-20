//! The single model-provider host one private Ask run may reach.
//!
//! The proxy needs a destination before the child exists, and that destination
//! has to come from the *selected runtime's own configuration* rather than from
//! the request: a caller who could name the provider could name anything.
//!
//! Claude's host is fixed by the CLI it is. Hermes chooses its provider in the
//! profile the attempt stages, so the host is derived from that profile's
//! configured provider name through an explicit table. An unrecognised provider
//! is refused, not guessed — a wrong host would either break every run or, far
//! worse, bound egress to somewhere that is not the provider while still
//! looking certified.

use super::egress_proxy::ProviderHost;
use super::PrivateAskFailure;

/// Providers this adapter can bound, by the name Hermes uses in its profile.
///
/// The table is deliberately short and literal. Every entry is an HTTPS API
/// endpoint on 443, which is the only shape the CONNECT proxy speaks.
const HERMES_PROVIDER_HOSTS: &[(&str, &str)] = &[
    ("anthropic", "api.anthropic.com"),
    ("openai", "api.openai.com"),
    ("openrouter", "openrouter.ai"),
];

/// The provider host for one selected runtime.
pub(super) fn provider_host(
    runtime_id: &str,
    hermes_provider: Option<&str>,
) -> Result<ProviderHost, PrivateAskFailure> {
    match runtime_id {
        "claude" => ProviderHost::parse("api.anthropic.com"),
        "hermes" => {
            let configured = hermes_provider
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or(PrivateAskFailure::EgressBoundUnverified)?
                .to_ascii_lowercase();
            let host = HERMES_PROVIDER_HOSTS
                .iter()
                .find(|(name, _)| *name == configured)
                .map(|(_, host)| *host)
                .ok_or(PrivateAskFailure::EgressBoundUnverified)?;
            ProviderHost::parse(host)
        }
        _ => Err(PrivateAskFailure::MissingRuntime),
    }
}

/// The provider name configured in one staged Hermes profile directory.
///
/// Read from the copy inside the run root, not from the source: the child runs
/// against the staged bytes, and deriving the bound destination from anything
/// else would bound the proxy to a provider the child is not using.
pub(super) fn staged_hermes_provider(profile_dir: &std::path::Path) -> Option<String> {
    let document =
        crate::managed_agents::hermes_profile_readiness::read_profile_yaml(profile_dir).ok()??;
    document
        .as_mapping()?
        .get("model")?
        .as_mapping()?
        .get("provider")?
        .as_str()
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_is_bound_to_the_anthropic_api_host() {
        assert_eq!(
            provider_host("claude", None).unwrap().as_str(),
            "api.anthropic.com"
        );
    }

    /// A hermes profile names its provider; the host comes from the table, and
    /// a provider the table does not know refuses the run. Removing the
    /// `ok_or(EgressBoundUnverified)` would leave a run with no bounded
    /// destination at all.
    #[test]
    fn a_hermes_provider_is_resolved_only_from_the_explicit_table() {
        assert_eq!(
            provider_host("hermes", Some("Anthropic")).unwrap().as_str(),
            "api.anthropic.com"
        );
        assert_eq!(
            provider_host("hermes", Some("openrouter"))
                .unwrap()
                .as_str(),
            "openrouter.ai"
        );
        for configured in [None, Some(""), Some("  "), Some("hpc"), Some("ollama")] {
            assert_eq!(
                provider_host("hermes", configured).unwrap_err(),
                PrivateAskFailure::EgressBoundUnverified,
                "accepted {configured:?}"
            );
        }
    }

    #[test]
    fn an_unknown_runtime_has_no_provider() {
        assert_eq!(
            provider_host("codex", None).unwrap_err(),
            PrivateAskFailure::MissingRuntime
        );
    }
}
