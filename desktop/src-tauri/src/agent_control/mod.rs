//! Agent desktop control endpoint (#197). Crew-owned.

mod bridge_js;
mod commands;
mod flight;
mod instruments;
mod lease;
mod live;
mod origin;
mod overlay;
mod protocol;
mod runtime;
mod server;
mod snapshot;
mod token;

#[cfg(test)]
mod tests;

pub use commands::*;
pub use server::{spawn_agent_control_on, AgentControlHandle};
