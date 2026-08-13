//! Engine errors.

use thiserror::Error;

/// Recoverable wiki engine failure.
#[derive(Debug, Error)]
pub enum WikiError {
    /// Steering file exists but is not valid JSON / schema.
    #[error("invalid steering file: {0}")]
    InvalidSteering(String),
    /// Git snapshot failed.
    #[error("git: {0}")]
    Git(String),
    /// Generation failed (LLM or heuristic).
    #[error("generate: {0}")]
    Generate(String),
    /// Publish / event build failed.
    #[error("publish: {0}")]
    Publish(String),
    /// Ask grounding or synthesis failed.
    #[error("ask: {0}")]
    Ask(String),
    /// Governance: another generate is already running for this repo.
    #[error("generate already in progress")]
    GenerateInProgress,
    /// Requested wiki or page was not found.
    #[error("not found")]
    NotFound,
}

impl WikiError {
    /// Closed-taxonomy MCP error code (#197 style).
    pub fn code(&self) -> &'static str {
        match self {
            Self::GenerateInProgress => "generate_in_progress",
            Self::NotFound => "not_found",
            Self::Git(_) => "git_error",
            Self::InvalidSteering(_) => "invalid_steering",
            Self::Generate(_) => "generate_failed",
            Self::Publish(_) => "publish_failed",
            Self::Ask(_) => "ask_failed",
        }
    }
}
