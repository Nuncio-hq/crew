//! Serde contracts for agent-directed human questions.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The ACP engine which originated a question request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// Claude Code.
    Claude,
    /// OpenAI Codex.
    Codex,
    /// Another ACP-compatible engine.
    Other(String),
}

/// A selectable option in an agent-directed question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Option_ {
    /// Engine-native wire value supplied by the schema's `const`.
    pub value: String,
    /// Human-facing display label supplied by the schema's `title`.
    pub label: String,
    /// Human-readable option explanation supplied by the schema's `description`.
    pub description: String,
}

/// One question presented to the human.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInputQuestion {
    /// Crew-owned stable identifier.
    pub id: String,
    /// Short heading shown by clients.
    pub header: String,
    /// The question text.
    pub question: String,
    /// Selectable options.
    pub options: Vec<Option_>,
    /// Whether multiple options may be selected.
    #[serde(default)]
    pub multi_select: bool,
    /// Whether a custom answer is accepted.
    #[serde(default)]
    pub allow_custom_answer: bool,
    /// Whether notes may accompany a selection.
    #[serde(default)]
    pub allow_notes: bool,
    /// Whether an answer is required by the originating engine schema.
    #[serde(default)]
    pub required: bool,
}

/// A durable question request published to a channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserInputRequest {
    /// Unique request identifier.
    pub request_id: String,
    /// ACP session identifier.
    pub session_id: String,
    /// Harness turn identifier.
    pub turn_id: String,
    /// NIP-29 channel identifier.
    pub channel_id: String,
    /// ACP tool call identifier, when supplied.
    pub tool_call_id: Option<String>,
    /// Originating engine.
    pub engine: Engine,
    /// Optional human-facing context.
    pub message: Option<String>,
    /// Questions in client-facing stable-ID form.
    pub questions: Vec<UserInputQuestion>,
}

/// A normalized answer value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserInputAnswer {
    /// A single answer.
    Text(String),
    /// Multiple selected answers.
    Multi(Vec<String>),
    /// A selection with per-choice notes.
    Structured {
        /// Selected values.
        selected: UserInputSelection,
        /// Notes keyed by selected value.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        choice_notes: BTreeMap<String, String>,
    },
    /// The human explicitly skipped the question.
    Skipped,
}

/// Selection payload used by structured answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserInputSelection {
    /// One selected value.
    One(String),
    /// Multiple selected values.
    Many(Vec<String>),
}

/// Answers keyed by Crew-owned question IDs.
pub type UserInputAnswers = BTreeMap<String, Option<UserInputAnswer>>;

/// Outcome of a durable agent question request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserInputResolutionOutcome {
    /// The owner supplied an answer.
    Answered,
    /// The owner explicitly answered nothing.
    Declined,
    /// The turn or harness terminated before an answer was received.
    Cancelled,
}

/// Terminal resolution of a durable agent question request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInputResolved {
    /// Event id of the corresponding kind-46040 request.
    pub request_event_id: String,
    /// Terminal outcome.
    pub outcome: UserInputResolutionOutcome,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_contract_uses_stable_wire_vocabulary() {
        let value = UserInputResolved {
            request_event_id: "a".repeat(64),
            outcome: UserInputResolutionOutcome::Cancelled,
        };
        assert_eq!(
            serde_json::to_value(value).expect("serialize resolution"),
            serde_json::json!({
                "request_event_id": "a".repeat(64),
                "outcome": "cancelled"
            })
        );
    }

    #[test]
    fn question_required_defaults_for_legacy_events() {
        let question: UserInputQuestion =
            serde_json::from_str(r#"{"id":"q0","header":"Pick","question":"Choose","options":[]}"#)
                .expect("deserialize legacy question");
        assert!(!question.required);
    }
}
