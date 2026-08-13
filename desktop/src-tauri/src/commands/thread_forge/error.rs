use serde::Serialize;

use super::super::gh_cli::GhUnavailable;
use super::types::ForgeAvailability;

/// Why a forge probe or write could not complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForgeError {
    CliMissing,
    CliFailed(String),
    RateLimited { reset_at: Option<String> },
    NotFound(String),
    InvalidInput(String),
}

impl ForgeError {
    pub fn availability(&self) -> super::types::ForgeAvailability {
        match self {
            Self::CliMissing => ForgeAvailability::CliMissing,
            Self::RateLimited { .. } => ForgeAvailability::RateLimited,
            Self::CliFailed(_) | Self::NotFound(_) | Self::InvalidInput(_) => {
                ForgeAvailability::CliFailed
            }
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::CliMissing => "Forge CLI was not found.".to_string(),
            Self::CliFailed(message) | Self::NotFound(message) | Self::InvalidInput(message) => {
                message.clone()
            }
            Self::RateLimited { reset_at } => match reset_at {
                Some(reset) => format!("Forge API rate limit reached. Retry after {reset}."),
                None => "Forge API rate limit reached.".to_string(),
            },
        }
    }

    pub fn rate_limited_until(&self) -> Option<String> {
        match self {
            Self::RateLimited { reset_at } => reset_at.clone(),
            _ => None,
        }
    }
}

impl From<GhUnavailable> for ForgeError {
    fn from(_: GhUnavailable) -> Self {
        Self::CliMissing
    }
}

impl From<ForgeError> for String {
    fn from(error: ForgeError) -> Self {
        error.message()
    }
}

/// Classify `gh` stdout/stderr without inventing GraphQL fields.
pub fn classify_gh_output(stdout: &str, stderr: &str, success: bool) -> Result<(), ForgeError> {
    let blob = format!("{stderr}\n{stdout}");
    let lower = blob.to_ascii_lowercase();
    if lower.contains("rate limit") || blob.contains("RATE_LIMITED") {
        let reset_at = extract_reset_at(&blob);
        return Err(ForgeError::RateLimited { reset_at });
    }
    if !success {
        let message = first_nonempty(stderr, stdout)
            .unwrap_or("Forge CLI command failed.")
            .to_string();
        return Err(ForgeError::CliFailed(truncate_message(message)));
    }
    if let Some(errors) = graphql_error_messages(stdout) {
        if errors.iter().any(|error| error.contains("RATE_LIMITED")) {
            return Err(ForgeError::RateLimited {
                reset_at: extract_reset_at(stdout),
            });
        }
        return Err(ForgeError::CliFailed(truncate_message(errors.join("; "))));
    }
    Ok(())
}

fn graphql_error_messages(stdout: &str) -> Option<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(stdout).ok()?;
    let errors = value.get("errors")?.as_array()?;
    if errors.is_empty() {
        return None;
    }
    Some(
        errors
            .iter()
            .filter_map(|error| {
                let ty = error.get("type").and_then(|value| value.as_str());
                let message = error.get("message").and_then(|value| value.as_str())?;
                Some(match ty {
                    Some(ty) => format!("{ty}: {message}"),
                    None => message.to_string(),
                })
            })
            .collect(),
    )
}

fn extract_reset_at(blob: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(blob) {
        if let Some(reset) = value
            .pointer("/data/rateLimit/resetAt")
            .and_then(|value| value.as_str())
        {
            return Some(reset.to_string());
        }
    }
    blob.lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix("resetAt:")
                .or_else(|| line.strip_prefix("X-RateLimit-Reset:"))
                .map(|value| value.trim().to_string())
        })
        .filter(|value| !value.is_empty())
}

fn first_nonempty<'a>(a: &'a str, b: &'a str) -> Option<&'a str> {
    let a = a.trim();
    if !a.is_empty() {
        return Some(a);
    }
    let b = b.trim();
    if b.is_empty() {
        None
    } else {
        Some(b)
    }
}

fn truncate_message(message: String) -> String {
    const MAX: usize = 400;
    if message.len() <= MAX {
        message
    } else {
        format!("{}…", &message[..MAX])
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeActionResult {
    pub ok: bool,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_from_graphql_type() {
        let stdout = r#"{"errors":[{"type":"RATE_LIMITED","message":"API rate limit exceeded"}]}"#;
        let err = classify_gh_output(stdout, "", true).expect_err("rate limit");
        assert!(matches!(err, ForgeError::RateLimited { .. }));
        assert_eq!(err.availability(), ForgeAvailability::RateLimited);
    }

    #[test]
    fn cli_failed_from_stderr() {
        let err = classify_gh_output("", "gh: Not Found", false).expect_err("fail");
        assert!(matches!(err, ForgeError::CliFailed(_)));
    }

    #[test]
    fn success_with_empty_errors_is_ok() {
        classify_gh_output(r#"{"data":{}}"#, "", true).expect("ok");
    }
}
