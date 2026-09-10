//! Bulk canvas recovery over Buzz events and the shared owner operation store.

mod cleanup;
mod driver;
mod guard;
mod native;
mod prepare;
mod record;
mod save_lock;
mod service;
mod worker;
#[allow(unused_imports)] // Consumed by the separately integrated durable deletion coordinator.
pub(crate) use cleanup::save_channel_crew_member_cleanup;
#[allow(unused_imports)] // Public crate contract for the deletion coordinator.
pub(crate) use record::{Outcome as CrewSaveOutcome, Progress as CrewSaveProgress};

pub(crate) use service::{list, retry, save, status};
pub(crate) use worker::start;

#[cfg(all(test, unix))]
mod tests;

/// Result shared by role editing and the durable member deletion coordinator.
#[derive(serde::Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
// Keep the progress payload inline so the existing tagged IPC response shape
// remains unchanged for the renderer and deletion coordinator.
#[allow(clippy::large_enum_variant)]
pub(crate) enum CrewSaveResult {
    Unchanged { current_event_id: Option<String> },
    Conflict { current_event_id: Option<String> },
    ReviewRequired { current_event_id: Option<String> },
    RecoveryPending { operation_id: String },
    Saved { progress: record::Progress },
}
