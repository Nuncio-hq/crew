//! ACP form elicitation normalization and answer reconstruction.

use std::collections::BTreeMap;
use std::sync::Arc;

use buzz_core::{
    kind::KIND_AGENT_USER_INPUT_ANSWER,
    user_input::{
        Engine, Option_, UserInputAnswer, UserInputAnswers, UserInputQuestion, UserInputRequest,
        UserInputSelection,
    },
};
use nostr::{Alphabet, Keys, SingleLetterTag, TagKind};
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

use crate::relay::{BuzzEvent, RelayEventPublisher, RestClient};
use crate::{is_owner_or_sibling, OwnerCache};

/// Engine field mapping retained while a form is pending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldMapping {
    /// Crew question identifier.
    pub id: String,
    /// Native ACP content key.
    pub field_key: String,
    /// Native custom-answer key, when present.
    pub custom_key: Option<String>,
    /// Whether the native field accepts an array.
    pub multi_select: bool,
}

/// Normalized ACP form and its native field mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedForm {
    /// Client-facing questions.
    pub questions: Vec<UserInputQuestion>,
    /// Native reconstruction map.
    pub mappings: Vec<FieldMapping>,
}

/// Shared transport for durable user-input requests and owner-authored answers.
pub(crate) struct QuestionRuntime {
    publisher: RelayEventPublisher,
    keys: Keys,
    owner_cache: Arc<OwnerCache>,
    rest_client: RestClient,
    pending: Mutex<std::collections::HashMap<String, oneshot::Sender<Option<UserInputAnswers>>>>,
}

impl QuestionRuntime {
    pub(crate) fn new(
        publisher: RelayEventPublisher,
        keys: Keys,
        owner_cache: Arc<OwnerCache>,
        rest_client: RestClient,
    ) -> Arc<Self> {
        Arc::new(Self {
            publisher,
            keys,
            owner_cache,
            rest_client,
            pending: Mutex::new(std::collections::HashMap::new()),
        })
    }

    pub(crate) async fn publish_and_wait(
        &self,
        channel_id: Uuid,
        session_id: &str,
        turn_id: &str,
        engine: Engine,
        form: NormalizedForm,
        request_id: &str,
    ) -> Result<Option<UserInputAnswers>, String> {
        let request = UserInputRequest {
            request_id: request_id.to_string(),
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            channel_id: channel_id.to_string(),
            tool_call_id: None,
            engine,
            message: None,
            questions: form.questions,
            native: buzz_core::user_input::Native {
                data: serde_json::Map::new(),
            },
        };
        let content = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        let builder = buzz_sdk::build_agent_user_input_request(channel_id, &content)
            .map_err(|e| e.to_string())?;
        let event = builder
            .sign_with_keys(&self.keys)
            .map_err(|e| e.to_string())?;
        let event_id = event.id.to_hex();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(event_id.clone(), tx);
        if let Err(error) = self.publisher.publish_event(event).await {
            self.pending.lock().await.remove(&event_id);
            return Err(error.to_string());
        }
        match rx.await {
            Ok(answers) => Ok(answers),
            Err(_) => Err("pending user-input request was cancelled".to_string()),
        }
    }

    #[allow(dead_code)]
    pub(crate) async fn cancel_all(&self) {
        let mut pending = self.pending.lock().await;
        for (_, sender) in pending.drain() {
            let _ = sender.send(None);
        }
    }

    pub(crate) async fn handle_event(&self, buzz_event: &BuzzEvent) {
        if buzz_event.event.kind.as_u16() as u32 != KIND_AGENT_USER_INPUT_ANSWER {
            return;
        }
        let request_event_id = buzz_event
            .event
            .tags
            .iter()
            .find(|tag| {
                tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E))
            })
            .and_then(|tag| tag.content().map(str::to_owned));
        let Some(request_event_id) = request_event_id else {
            return;
        };
        if !is_owner_or_sibling(
            &buzz_event.event.pubkey.to_hex(),
            &self.owner_cache,
            &self.rest_client,
        )
        .await
        {
            tracing::warn!(
                author = %buzz_event.event.pubkey,
                request_event_id,
                "ignoring non-owner user-input answer"
            );
            return;
        }
        let answers = match serde_json::from_str::<UserInputAnswers>(&buzz_event.event.content) {
            Ok(answers) => answers,
            Err(error) => {
                tracing::warn!(%error, "ignoring malformed user-input answer");
                return;
            }
        };
        let sender = self.pending.lock().await.remove(&request_event_id);
        if let Some(sender) = sender {
            let _ = sender.send(Some(answers));
        } else {
            tracing::debug!(request_event_id, "ignoring late user-input answer");
        }
    }
}

fn option(value: &serde_json::Value) -> Option<Option_> {
    let object = value.as_object()?;
    let label = object.get("const")?.as_str()?.to_owned();
    Some(Option_ {
        label,
        description: object
            .get("description")
            .or_else(|| object.get("title"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

/// Normalize the supported ACP form subset into Crew's contract.
pub(crate) fn normalize_form(schema: &serde_json::Value, engine: Engine) -> Option<NormalizedForm> {
    let properties = schema.get("properties")?.as_object()?;
    let required = schema
        .get("required")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let is_codex = matches!(engine, Engine::Codex);
    let mut questions = Vec::new();
    let mut mappings = Vec::new();

    for (key, field) in properties {
        if key.ends_with("_custom") {
            continue;
        }
        let index = questions.len();
        let object = field.as_object()?;
        let (options, multi_select) = if let Some(values) = object.get("oneOf") {
            let values = values.as_array()?;
            (
                values.iter().map(option).collect::<Option<Vec<_>>>()?,
                false,
            )
        } else if let Some(items) = object.get("items") {
            let values = items.get("anyOf")?.as_array()?;
            (values.iter().map(option).collect::<Option<Vec<_>>>()?, true)
        } else if object.get("type").and_then(serde_json::Value::as_str) == Some("string") {
            (Vec::new(), false)
        } else {
            return None;
        };
        let id = format!("q{index}");
        let custom_key = properties
            .contains_key(&format!("{key}_custom"))
            .then(|| format!("{key}_custom"));
        let question = UserInputQuestion {
            id: id.clone(),
            header: object
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(key)
                .to_owned(),
            question: object
                .get("description")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(key)
                .to_owned(),
            options,
            multi_select,
            allow_custom_answer: custom_key.is_some(),
            allow_notes: false,
        };
        if is_codex && !required.is_empty() && !required.contains(key.as_str()) {
            // Optional fields are still representable; preserve the schema
            // without inventing a value. The required set is carried by the
            // native map in the caller when it needs engine-specific policy.
        }
        questions.push(question);
        mappings.push(FieldMapping {
            id,
            field_key: key.clone(),
            custom_key,
            multi_select,
        });
    }
    (!questions.is_empty()).then_some(NormalizedForm {
        questions,
        mappings,
    })
}

/// Rebuild ACP content using native field keys.
#[allow(dead_code)]
pub(crate) fn reconstruct_content(
    form: &NormalizedForm,
    answers: &BTreeMap<String, Option<UserInputAnswer>>,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut content = serde_json::Map::new();
    for mapping in &form.mappings {
        let answer = answers.get(&mapping.id).cloned().flatten()?;
        let (key, value) = match answer {
            UserInputAnswer::Text(value) => {
                if let Some(custom_key) = &mapping.custom_key {
                    (custom_key.clone(), serde_json::Value::String(value))
                } else {
                    (mapping.field_key.clone(), serde_json::Value::String(value))
                }
            }
            UserInputAnswer::Multi(values) => (
                mapping.field_key.clone(),
                serde_json::Value::Array(
                    values.into_iter().map(serde_json::Value::String).collect(),
                ),
            ),
            UserInputAnswer::Structured {
                selected,
                choice_notes,
            } => {
                let selected = match selected {
                    UserInputSelection::One(value) => vec![value],
                    UserInputSelection::Many(values) => values,
                };
                let value = if mapping.multi_select {
                    serde_json::Value::Array(
                        selected
                            .into_iter()
                            .map(serde_json::Value::String)
                            .collect(),
                    )
                } else {
                    serde_json::Value::String(selected.into_iter().next()?)
                };
                if let Some(custom_key) = &mapping.custom_key {
                    if !choice_notes.is_empty() {
                        (
                            custom_key.clone(),
                            serde_json::Value::String(
                                choice_notes.values().next().cloned().unwrap_or_default(),
                            ),
                        )
                    } else {
                        (mapping.field_key.clone(), value)
                    }
                } else {
                    (mapping.field_key.clone(), value)
                }
            }
            UserInputAnswer::Skipped => return None,
        };
        content.insert(key, value);
    }
    Some(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_select_and_freeform() {
        let schema = serde_json::json!({"type":"object","properties":{
            "question_0":{"type":"string","title":"Pick","oneOf":[
                {"const":"yes","title":"Yes","description":"Do it"},
                {"const":"no","title":"No","description":"Don't"}
            ]},
            "question_1":{"type":"string","description":"Why?"}
        }});
        let form = normalize_form(&schema, Engine::Claude).expect("supported");
        assert_eq!(form.questions[0].options[0].label, "yes");
        assert!(form.questions[1].options.is_empty());
    }

    #[test]
    fn reconstructs_custom_and_multi_select() {
        let schema = serde_json::json!({"type":"object","properties":{
            "question_0":{"type":"string","oneOf":[{"const":"a"}]},
            "question_0_custom":{"type":"string"},
            "question_1":{"type":"array","items":{"anyOf":[{"const":"x"}]}}
        }});
        let form = normalize_form(&schema, Engine::Claude).expect("supported");
        let answers = BTreeMap::from([
            ("q0".into(), Some(UserInputAnswer::Text("custom".into()))),
            ("q1".into(), Some(UserInputAnswer::Multi(vec!["x".into()]))),
        ]);
        let content = reconstruct_content(&form, &answers).expect("answers");
        assert_eq!(content["question_0_custom"], "custom");
        assert_eq!(content["question_1"], serde_json::json!(["x"]));
    }

    #[test]
    fn rejects_unsupported_schema() {
        assert!(normalize_form(
            &serde_json::json!({"type":"object","properties":{"x":{"type":"number"}}}),
            Engine::Codex
        )
        .is_none());
    }
}
