//! ACP form elicitation normalization and answer reconstruction.

use std::collections::BTreeMap;
use std::sync::Arc;

use buzz_core::{
    kind::KIND_AGENT_USER_INPUT_ANSWER,
    user_input::{
        Engine, Option_, UserInputAnswer, UserInputAnswers, UserInputQuestion, UserInputRequest,
        UserInputResolutionOutcome, UserInputResolved, UserInputSelection,
    },
};
use nostr::{Alphabet, Keys, SingleLetterTag, TagKind};
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

use crate::relay::{BuzzEvent, RelayEventPublisher, RestClient};
use crate::OwnerCache;

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
    /// Whether the native field is required by the engine schema.
    pub required: bool,
}

/// Normalized ACP form and its native field mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedForm {
    /// Client-facing questions.
    pub questions: Vec<UserInputQuestion>,
    /// Native reconstruction map.
    pub mappings: Vec<FieldMapping>,
}

/// A published elicitation whose answer is still pending.
pub(crate) struct PendingQuestion {
    /// ACP JSON-RPC request identifier.
    pub request_id: serde_json::Value,
    /// Normalized form used to rebuild the engine response.
    pub form: NormalizedForm,
    /// Receiver resolved by the relay answer router or cancellation.
    pub receiver: oneshot::Receiver<Option<UserInputAnswers>>,
}

/// Shared transport for durable user-input requests and owner-authored answers.
pub(crate) struct QuestionRuntime {
    publisher: RelayEventPublisher,
    keys: Keys,
    owner_cache: Arc<OwnerCache>,
    pending: Mutex<std::collections::HashMap<String, PendingRequest>>,
}

struct PendingRequest {
    channel_id: Uuid,
    sender: oneshot::Sender<Option<UserInputAnswers>>,
}

fn answer_author_is_intended_owner(author: &str, owner_cache: &OwnerCache) -> bool {
    owner_cache.get() == Some(author)
}

impl QuestionRuntime {
    pub(crate) fn new(
        publisher: RelayEventPublisher,
        keys: Keys,
        owner_cache: Arc<OwnerCache>,
        _rest_client: RestClient,
    ) -> Arc<Self> {
        Arc::new(Self {
            publisher,
            keys,
            owner_cache,
            pending: Mutex::new(std::collections::HashMap::new()),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn publish(
        &self,
        channel_id: Uuid,
        session_id: &str,
        turn_id: &str,
        engine: Engine,
        form: NormalizedForm,
        request_id: &str,
        message: Option<&str>,
        tool_call_id: Option<&str>,
    ) -> Result<(String, oneshot::Receiver<Option<UserInputAnswers>>), String> {
        let request = UserInputRequest {
            request_id: request_id.to_string(),
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            channel_id: channel_id.to_string(),
            tool_call_id: tool_call_id.map(str::to_owned),
            engine,
            message: message.map(str::to_owned),
            questions: form.questions,
        };
        let content = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        let owner_pubkey = self
            .owner_cache
            .get()
            .ok_or_else(|| "agent owner is required for durable user input".to_string())?;
        let builder = buzz_sdk::build_agent_user_input_request(channel_id, owner_pubkey, &content)
            .map_err(|e| e.to_string())?;
        let event = builder
            .sign_with_keys(&self.keys)
            .map_err(|e| e.to_string())?;
        let event_id = event.id.to_hex();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(
            event_id.clone(),
            PendingRequest {
                channel_id,
                sender: tx,
            },
        );
        if let Err(error) = self.publisher.publish_event(event).await {
            self.pending.lock().await.remove(&event_id);
            return Err(error.to_string());
        }
        Ok((event_id, rx))
    }

    pub(crate) async fn cancel(&self, event_id: &str) {
        if let Some(pending) = self.pending.lock().await.remove(event_id) {
            let _ = pending.sender.send(None);
            self.publish_resolution(
                pending.channel_id,
                event_id,
                UserInputResolutionOutcome::Cancelled,
            )
            .await;
        }
    }

    /// Resolve every request still owned by this runtime during graceful shutdown.
    pub(crate) async fn shutdown_pending(&self) {
        let pending = {
            let mut guard = self.pending.lock().await;
            std::mem::take(&mut *guard)
        };
        for (event_id, pending) in pending {
            let _ = pending.sender.send(None);
            self.publish_resolution(
                pending.channel_id,
                &event_id,
                UserInputResolutionOutcome::Cancelled,
            )
            .await;
        }
    }

    async fn publish_resolution(
        &self,
        channel_id: Uuid,
        request_event_id: &str,
        outcome: UserInputResolutionOutcome,
    ) {
        let content = match serde_json::to_string(&UserInputResolved {
            request_event_id: request_event_id.to_owned(),
            outcome,
        }) {
            Ok(content) => content,
            Err(error) => {
                tracing::warn!(%error, request_event_id, "failed to serialize user-input resolution");
                return;
            }
        };
        let builder = match buzz_sdk::build_agent_user_input_resolved(
            channel_id,
            request_event_id,
            &content,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                tracing::warn!(%error, request_event_id, "failed to build user-input resolution");
                return;
            }
        };
        match builder.sign_with_keys(&self.keys) {
            Ok(event) => {
                if let Err(error) = self.publisher.publish_event(event).await {
                    tracing::warn!(%error, request_event_id, "failed to publish user-input resolution");
                }
            }
            Err(error) => {
                tracing::warn!(%error, request_event_id, "failed to sign user-input resolution");
            }
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
        if !answer_author_is_intended_owner(&buzz_event.event.pubkey.to_hex(), &self.owner_cache) {
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
        let pending = self.pending.lock().await.remove(&request_event_id);
        if let Some(pending) = pending {
            let declined = answers.values().all(Option::is_none);
            let _ = pending.sender.send(Some(answers));
            self.publish_resolution(
                pending.channel_id,
                &request_event_id,
                if declined {
                    UserInputResolutionOutcome::Declined
                } else {
                    UserInputResolutionOutcome::Answered
                },
            )
            .await;
        } else {
            tracing::debug!(request_event_id, "ignoring late user-input answer");
        }
    }
}

fn option(value: &serde_json::Value) -> Option<Option_> {
    let object = value.as_object()?;
    let value = object.get("const")?.as_str()?.to_owned();
    Some(Option_ {
        value: value.clone(),
        label: object
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&value)
            .to_owned(),
        description: object
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

fn natural_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    let mut l = left.chars().peekable();
    let mut r = right.chars().peekable();
    loop {
        match (l.peek(), r.peek()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(a), Some(b)) if a.is_ascii_digit() && b.is_ascii_digit() => {
                let mut ln = String::new();
                let mut rn = String::new();
                while l.peek().is_some_and(char::is_ascii_digit) {
                    ln.push(l.next().unwrap_or_default());
                }
                while r.peek().is_some_and(char::is_ascii_digit) {
                    rn.push(r.next().unwrap_or_default());
                }
                let ordering = ln
                    .parse::<u64>()
                    .unwrap_or(u64::MAX)
                    .cmp(&rn.parse::<u64>().unwrap_or(u64::MAX));
                if ordering != std::cmp::Ordering::Equal {
                    return ordering;
                }
            }
            (Some(a), Some(b)) => {
                let ordering = a.cmp(b);
                l.next();
                r.next();
                if ordering != std::cmp::Ordering::Equal {
                    return ordering;
                }
            }
        }
    }
}

/// Normalize the supported ACP form subset into Crew's contract.
pub(crate) fn normalize_form(schema: &serde_json::Value) -> Option<NormalizedForm> {
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
    let mut questions = Vec::new();
    let mut mappings = Vec::new();

    let mut property_keys = properties.keys().collect::<Vec<_>>();
    property_keys.sort_by(|left, right| natural_cmp(left, right));
    for key in property_keys {
        let field = properties.get(key)?;
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
            required: required.contains(key.as_str()),
            // ACP has no notes concept; intentionally false until an engine
            // provides a notes affordance.
            allow_notes: false,
        };
        questions.push(question);
        mappings.push(FieldMapping {
            id,
            field_key: key.clone(),
            custom_key,
            multi_select,
            required: required.contains(key.as_str()),
        });
    }
    (!questions.is_empty()).then_some(NormalizedForm {
        questions,
        mappings,
    })
}

/// Rebuild ACP content using native field keys.
pub(crate) fn reconstruct_content(
    form: &NormalizedForm,
    answers: &BTreeMap<String, Option<UserInputAnswer>>,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut content = serde_json::Map::new();
    for mapping in &form.mappings {
        let question = form
            .questions
            .iter()
            .find(|question| question.id == mapping.id)?;
        let wire_value = |value: String| {
            question
                .options
                .iter()
                .find(|option| option.label == value || option.value == value)
                .map(|option| option.value.clone())
                .unwrap_or(value)
        };
        let answer = match answers.get(&mapping.id) {
            Some(Some(UserInputAnswer::Skipped)) | Some(None) | None if !mapping.required => {
                continue
            }
            Some(Some(UserInputAnswer::Skipped)) | Some(None) | None => return None,
            Some(Some(answer)) => answer.clone(),
        };
        let (key, value) = match answer {
            UserInputAnswer::Text(value) => {
                let matches_option = question
                    .options
                    .iter()
                    .any(|option| option.value == value || option.label == value);
                if matches_option {
                    (
                        mapping.field_key.clone(),
                        serde_json::Value::String(wire_value(value)),
                    )
                } else if let Some(custom_key) = &mapping.custom_key {
                    (custom_key.clone(), serde_json::Value::String(value))
                } else {
                    (
                        mapping.field_key.clone(),
                        serde_json::Value::String(wire_value(value)),
                    )
                }
            }
            UserInputAnswer::Multi(values) => (
                mapping.field_key.clone(),
                serde_json::Value::Array(
                    values
                        .into_iter()
                        .map(wire_value)
                        .map(serde_json::Value::String)
                        .collect(),
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
                let selected = selected.into_iter().map(wire_value).collect::<Vec<_>>();
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
    use tokio::sync::mpsc;

    #[test]
    fn user_input_answer_requires_the_intended_owner() {
        let owner_cache = OwnerCache::new(Some("owner".to_string()));
        assert!(answer_author_is_intended_owner("owner", &owner_cache));
        assert!(!answer_author_is_intended_owner(
            "same-owner-sibling",
            &owner_cache
        ));
    }

    #[test]
    fn normalizes_select_and_freeform() {
        let schema = serde_json::json!({"type":"object","properties":{
            "question_0":{"type":"string","title":"Pick","oneOf":[
                {"const":"yes","title":"Yes","description":"Do it"},
                {"const":"no","title":"No","description":"Don't"}
            ]},
            "question_1":{"type":"string","description":"Why?"}
        }});
        let form = normalize_form(&schema).expect("supported");
        assert_eq!(form.questions[0].options[0].value, "yes");
        assert_eq!(form.questions[0].options[0].label, "Yes");
        assert!(form.questions[1].options.is_empty());
    }

    #[test]
    fn reconstructs_custom_and_multi_select() {
        let schema = serde_json::json!({"type":"object","properties":{
            "question_0":{"type":"string","oneOf":[{"const":"a"}]},
            "question_0_custom":{"type":"string"},
            "question_1":{"type":"array","items":{"anyOf":[{"const":"x"}]}}
        }});
        let form = normalize_form(&schema).expect("supported");
        let answers = BTreeMap::from([
            ("q0".into(), Some(UserInputAnswer::Text("custom".into()))),
            ("q1".into(), Some(UserInputAnswer::Multi(vec!["x".into()]))),
        ]);
        let content = reconstruct_content(&form, &answers).expect("answers");
        assert_eq!(content["question_0_custom"], "custom");
        assert_eq!(content["question_1"], serde_json::json!(["x"]));
    }

    #[test]
    fn reconstructs_matching_plain_option_on_native_key() {
        let schema = serde_json::json!({"type":"object","properties":{
            "question_0":{"type":"string","oneOf":[
                {"const":"production","title":"Production"}
            ]},
            "question_0_custom":{"type":"string"}
        }});
        let form = normalize_form(&schema).expect("supported");
        let by_value = BTreeMap::from([(
            "q0".into(),
            Some(UserInputAnswer::Text("production".into())),
        )]);
        let by_label = BTreeMap::from([(
            "q0".into(),
            Some(UserInputAnswer::Text("Production".into())),
        )]);
        assert_eq!(
            reconstruct_content(&form, &by_value).expect("value")["question_0"],
            "production"
        );
        assert_eq!(
            reconstruct_content(&form, &by_label).expect("label")["question_0"],
            "production"
        );
        assert!(!reconstruct_content(&form, &by_value)
            .expect("value")
            .contains_key("question_0_custom"));
    }

    #[test]
    fn rejects_unsupported_schema() {
        assert!(normalize_form(&serde_json::json!({
            "type":"object",
            "properties":{"x":{"type":"number"}}
        }))
        .is_none());
    }

    #[test]
    fn preserves_natural_question_order() {
        let mut properties = serde_json::Map::new();
        for index in 0..12 {
            properties.insert(
                format!("question_{index}"),
                serde_json::json!({"type":"string","title":format!("Q{index}")}),
            );
        }
        let form = normalize_form(&serde_json::json!({"type":"object","properties":properties}))
            .expect("supported");
        let headers = form
            .questions
            .iter()
            .map(|question| question.header.as_str())
            .collect::<Vec<_>>();
        assert_eq!(headers[2], "Q2");
        assert_eq!(headers[10], "Q10");
    }

    #[test]
    fn omits_unanswered_optional_fields_but_requires_required_fields() {
        let schema = serde_json::json!({
            "type":"object",
            "properties":{
                "question_0":{"type":"string","title":"Required"},
                "question_1":{"type":"string","title":"Optional"}
            },
            "required":["question_0"]
        });
        let form = normalize_form(&schema).expect("supported");
        let partial = BTreeMap::from([("q0".into(), Some(UserInputAnswer::Text("answer".into())))]);
        assert!(reconstruct_content(&form, &partial).is_some());
        let missing_required =
            BTreeMap::from([("q1".into(), Some(UserInputAnswer::Text("answer".into())))]);
        assert!(reconstruct_content(&form, &missing_required).is_none());
    }

    #[test]
    fn required_round_trips_and_old_question_events_default_to_false() {
        let schema = serde_json::json!({
            "type":"object",
            "properties":{"question_0":{"type":"string"}},
            "required":["question_0"]
        });
        let form = normalize_form(&schema).expect("supported");
        assert!(form.questions[0].required);
        let encoded = serde_json::to_string(&form.questions[0]).expect("question JSON");
        assert!(
            serde_json::from_str::<UserInputQuestion>(&encoded)
                .expect("question round trip")
                .required
        );
        let old = r#"{"id":"q0","header":"Pick","question":"Choose","options":[]}"#;
        assert!(
            !serde_json::from_str::<UserInputQuestion>(old)
                .expect("old question JSON")
                .required
        );
    }

    #[tokio::test]
    async fn ignores_non_owner_then_accepts_first_owner_answer() {
        let channel_id = Uuid::new_v4();
        let owner = Keys::generate();
        let stranger = Keys::generate();
        let (publisher, mut published) = RelayEventPublisher::test_pair();
        let runtime = QuestionRuntime::new(
            publisher,
            owner.clone(),
            Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
            RestClient {
                http: reqwest::Client::new(),
                base_url: "http://127.0.0.1:0".to_string(),
                keys: owner.clone(),
                auth_tag_json: None,
            },
        );
        let form = normalize_form(&serde_json::json!({
            "type":"object",
            "properties":{"question_0":{"type":"string","oneOf":[{"const":"yes"}]}}
        }))
        .expect("supported");
        let (event_id, mut receiver) = runtime
            .publish(
                channel_id,
                "session",
                "turn",
                Engine::Claude,
                form,
                "request",
                Some("Choose"),
                Some("tool"),
            )
            .await
            .expect("publish");
        let request = published.recv().await.expect("request event");
        assert_eq!(event_id, request.id.to_hex());
        let request_content: UserInputRequest =
            serde_json::from_str(&request.content).expect("request contract");
        assert_eq!(request_content.message.as_deref(), Some("Choose"));
        assert_eq!(request_content.tool_call_id.as_deref(), Some("tool"));

        let stranger_answer =
            buzz_sdk::build_agent_user_input_answer(channel_id, &event_id, r#"{"q0":"stranger"}"#)
                .expect("answer builder")
                .sign_with_keys(&stranger)
                .expect("signed stranger answer");
        runtime
            .handle_event(&BuzzEvent {
                channel_id,
                event: stranger_answer,
            })
            .await;
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut receiver)
                .await
                .is_err()
        );

        let owner_answer =
            buzz_sdk::build_agent_user_input_answer(channel_id, &event_id, r#"{"q0":"owner"}"#)
                .expect("answer builder")
                .sign_with_keys(&owner)
                .expect("signed owner answer");
        runtime
            .handle_event(&BuzzEvent {
                channel_id,
                event: owner_answer,
            })
            .await;
        assert!(receiver.await.expect("owner answer received").is_some());

        let late_answer =
            buzz_sdk::build_agent_user_input_answer(channel_id, &event_id, r#"{"q0":"late"}"#)
                .expect("answer builder")
                .sign_with_keys(&owner)
                .expect("signed late answer");
        runtime
            .handle_event(&BuzzEvent {
                channel_id,
                event: late_answer,
            })
            .await;
    }

    #[tokio::test]
    async fn publishes_one_resolution_for_each_terminal_outcome() {
        async fn publish_request(
            channel_id: Uuid,
            owner: &Keys,
        ) -> (
            Arc<QuestionRuntime>,
            mpsc::Receiver<nostr::Event>,
            String,
            oneshot::Receiver<Option<UserInputAnswers>>,
        ) {
            let (publisher, published) = RelayEventPublisher::test_pair();
            let runtime = QuestionRuntime::new(
                publisher,
                owner.clone(),
                Arc::new(crate::OwnerCache::new(Some(owner.public_key().to_hex()))),
                RestClient {
                    http: reqwest::Client::new(),
                    base_url: "http://127.0.0.1:0".to_string(),
                    keys: owner.clone(),
                    auth_tag_json: None,
                },
            );
            let form = normalize_form(&serde_json::json!({
                "type":"object",
                "properties":{"question_0":{"type":"string"}}
            }))
            .expect("supported");
            let (event_id, receiver) = runtime
                .publish(
                    channel_id,
                    "session",
                    "turn",
                    Engine::Claude,
                    form,
                    "request",
                    None,
                    None,
                )
                .await
                .expect("publish");
            (runtime, published, event_id, receiver)
        }

        async fn resolution(published: &mut mpsc::Receiver<nostr::Event>) -> UserInputResolved {
            let _request = published.recv().await.expect("request event");
            let event = published.recv().await.expect("resolution event");
            assert_eq!(
                event.kind.as_u16() as u32,
                buzz_core::kind::KIND_AGENT_USER_INPUT_RESOLVED
            );
            serde_json::from_str(&event.content).expect("resolution contract")
        }

        let channel_id = Uuid::new_v4();
        let owner = Keys::generate();

        let (runtime, mut published, event_id, receiver) =
            publish_request(channel_id, &owner).await;
        let answer =
            buzz_sdk::build_agent_user_input_answer(channel_id, &event_id, r#"{"q0":"answer"}"#)
                .expect("answer builder")
                .sign_with_keys(&owner)
                .expect("answer signature");
        runtime
            .handle_event(&BuzzEvent {
                channel_id,
                event: answer,
            })
            .await;
        assert!(receiver.await.expect("answer received").is_some());
        assert_eq!(
            resolution(&mut published).await.outcome,
            UserInputResolutionOutcome::Answered
        );

        let (runtime, mut published, event_id, receiver) =
            publish_request(channel_id, &owner).await;
        let decline =
            buzz_sdk::build_agent_user_input_answer(channel_id, &event_id, r#"{"q0":null}"#)
                .expect("answer builder")
                .sign_with_keys(&owner)
                .expect("answer signature");
        runtime
            .handle_event(&BuzzEvent {
                channel_id,
                event: decline,
            })
            .await;
        assert!(receiver.await.expect("decline received").is_some());
        assert_eq!(
            resolution(&mut published).await.outcome,
            UserInputResolutionOutcome::Declined
        );

        let (runtime, mut published, event_id, receiver) =
            publish_request(channel_id, &owner).await;
        runtime.cancel(&event_id).await;
        assert!(receiver.await.expect("cancel received").is_none());
        assert_eq!(
            resolution(&mut published).await.outcome,
            UserInputResolutionOutcome::Cancelled
        );

        let (runtime, mut published, _event_id, receiver) =
            publish_request(channel_id, &owner).await;
        runtime.shutdown_pending().await;
        assert!(receiver.await.expect("shutdown received").is_none());
        assert_eq!(
            resolution(&mut published).await.outcome,
            UserInputResolutionOutcome::Cancelled
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), published.recv())
                .await
                .is_err()
        );
    }
}
