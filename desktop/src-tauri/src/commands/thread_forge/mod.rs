//! Forge-neutral thread pull-request hub (Crew #193).
//!
//! The provider trait is implemented by GitHub wrapping `gh`. Types and
//! command payloads stay forge-neutral so a later GitLab/`glab` impl can
//! reuse them. Do not put host-specific strings on the trait or serde types.

mod commands;
mod diff;
mod error;
mod github;
mod graphql;
mod log_tail;
mod provider;
mod types;

pub use commands::*;
pub use types::*;
