//! Bounded local diagnostics for one Desktop-owned harness generation.
//!
//! This is a process sidechannel, not a relay event or process authority.
mod lease;
pub use lease::TransportLease;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Maximum flat-directory entries, excluding the shared empty lock file.
pub const MAX_DIRECTORY_ENTRIES: usize = 192;
/// Latest record plus two atomic-write staging names reserved per generation.
pub const GENERATION_ENTRY_RESERVATION: usize = 3;

/// Actionable refusal shared by native preflight and the harness writer.
pub const STORAGE_REVIEW_ERROR: &str = "Local transport history is full or unsafe. Stop the agent, review the .transport folder beside its log, remove only diagnostics for confirmed stopped processes, then retry.";

/// Opaque form of the existing canonical `(pubkey, relay)` runtime key.
/// The relay digest keeps URL query secrets out of local diagnostics.
pub fn runtime_id(pubkey: &str, relay_url: &str) -> Result<String, String> {
    if pubkey.len() != 64 || !pubkey.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("managed transport pubkey must be 64 hexadecimal characters".into());
    }
    let relay = crate::relay::normalize_relay_url(relay_url)
        .map_err(|_| "managed transport requires a valid relay identity")?;
    Ok(format!(
        "{}__{}",
        pubkey.to_ascii_lowercase(),
        hex::encode(Sha256::digest(relay.as_bytes()))
    ))
}

/// Maximum serialized size of one latest status record.
pub const MAX_RECORD_BYTES: u64 = 8192;
/// Writer liveness renewal cadence, independent of meaningful transitions.
pub const RENEWAL_SECONDS: u64 = 5;
/// Native readers expire a non-advancing live record after this interval.
pub const LEASE_SECONDS: u64 = 15;

/// Transport health is independent of the managed process lifecycle.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportState {
    /// No trustworthy local diagnostic is available.
    Unknown,
    /// Initial connection is in progress.
    Connecting,
    /// Authentication and connection recovery completed.
    Connected,
    /// An established connection is recovering within its burst budget.
    Degraded,
    /// The burst ended; background probes continue while the process lives.
    Exhausted,
    /// The relay explicitly denied the AUTH event for this attempt.
    AuthRejected,
}

/// Fixed diagnostic codes prevent relay text or credentials reaching disk.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportCode {
    /// No current transport error.
    None,
    /// A transport or handshake failed without credential evidence.
    ConnectionFailed,
    /// The bounded attempt or episode elapsed.
    Timeout,
    /// A correlated explicit AUTH denial requires configuration review.
    AuthDenied,
    /// The local writer or reader could not establish trustworthy status.
    StatusUnavailable,
}

impl TransportCode {
    /// Safe, fixed user-facing text; never derived from a server payload.
    pub fn message(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::ConnectionFailed => {
                Some("Relay connection unavailable. Check the relay and network.")
            }
            Self::Timeout => Some("Relay connection timed out. Check the relay and network."),
            Self::AuthDenied => {
                Some("Relay denied authentication. Review credentials and relay configuration.")
            }
            Self::StatusUnavailable => {
                Some("Local transport status unavailable. Retry by restarting the agent.")
            }
        }
    }
}

/// Latest transport substate projected into the existing runtime status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransportStatus {
    /// Typed connection state, separate from process and task state.
    pub state: TransportState,
    /// Fixed diagnostic code.
    pub code: TransportCode,
    /// Attempts consumed in the current health episode.
    pub attempts: u32,
    /// Monotonic elapsed health episode time, in milliseconds.
    pub elapsed_ms: u64,
    /// Wall-clock estimate of the next retry for display only.
    pub next_retry_at_ms: Option<u64>,
    /// Fixed safe error text, at most 1 KiB.
    pub last_error: Option<String>,
}

impl TransportStatus {
    /// Actionable fallback when local transport diagnostics cannot be trusted.
    pub fn unknown() -> Self {
        Self {
            state: TransportState::Unknown,
            code: TransportCode::StatusUnavailable,
            attempts: 0,
            elapsed_ms: 0,
            next_retry_at_ms: None,
            last_error: TransportCode::StatusUnavailable
                .message()
                .map(str::to_owned),
        }
    }

    /// Reject arbitrary server/error strings rather than truncating secrets.
    pub fn has_safe_error(&self) -> bool {
        self.last_error.as_deref() == self.code.message()
    }
}

/// Atomic latest record, accepted only for an already registered runtime key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransportRecord {
    /// Local protocol version; currently one.
    pub version: u32,
    /// Existing opaque runtime ID derived from canonical public key and relay.
    pub runtime_id: String,
    /// Unpredictable nonce assigned at managed process start.
    pub start_nonce: String,
    /// Strictly increasing sequence, including liveness renewals.
    pub sequence: u64,
    /// Wall-clock write timestamp for initial sanity checking only.
    pub timestamp_ms: u64,
    /// Startup finished unsuccessfully; this record may be retained on exit.
    pub terminal: bool,
    /// Transport diagnosis for this generation.
    pub transport: TransportStatus,
}

/// A native process binding and one bounded connection attempt carried by a
/// v2 record. The process id is an ACP echo; Desktop validates it against the
/// registered child before exposing the record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransportRecordV2 {
    /// Local protocol version.
    pub version: u32,
    /// Existing opaque runtime ID derived from canonical public key and relay.
    pub runtime_id: String,
    /// Unpredictable nonce assigned at managed process start.
    pub start_nonce: String,
    /// Strictly increasing record sequence, including liveness renewals.
    pub sequence: u64,
    /// Wall-clock write timestamp for initial sanity checking only.
    pub timestamp_ms: u64,
    /// Startup finished unsuccessfully; this record may be retained on exit.
    pub terminal: bool,
    /// Transport diagnosis for this generation.
    pub transport: TransportStatus,
    /// ACP's process-id echo, checked against the native registration.
    pub process_id: u32,
    /// Native pre-spawn lower bound echoed by ACP; this is not an OS start time.
    pub spawn_started_at_ms: u64,
    /// Current per-generation connection attempt, if one has started.
    pub connection_attempt: Option<TransportConnectionAttempt>,
    /// Exact negative AUTH acknowledgement received for the current attempt.
    pub received_auth: Option<TransportReceivedAuth>,
}

/// Immutable identity for one socket connection attempt within a generation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransportConnectionAttempt {
    /// Monotonic per-generation attempt sequence; it never resets on recovery.
    pub sequence: u64,
    /// Canonical nonzero UUID minted at socket-attempt start.
    pub id: String,
    /// Wall-clock attempt start timestamp.
    pub started_at_ms: u64,
}

/// Typed classification for an exact negative AUTH acknowledgement.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportAuthClassification {
    /// Relay's canonical blocked/banned denial class.
    CommunityBanned,
    /// A negative AUTH acknowledgement without a safe canonical class.
    OtherDenial,
}

/// Safe metadata for one exact negative AUTH acknowledgement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransportReceivedAuth {
    /// ID of the signed AUTH event sent by ACP.
    pub auth_event_id: String,
    /// UUID of the socket attempt that received the acknowledgement.
    pub attempt_id: String,
    /// Must equal the enclosing connection attempt sequence.
    pub attempt_sequence: u64,
    /// v2 evidence is emitted only for a negative exact ACK.
    pub accepted: bool,
    /// Safe fixed classification; relay text never enters this record.
    pub classification: TransportAuthClassification,
    /// Wall-clock time at which the exact ACK was received.
    pub received_at_ms: u64,
}

/// Decoded status record. v1 and v2 remain distinct strict wire schemas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportRecordEnvelope {
    /// Existing health-only record.
    V1(TransportRecord),
    /// Process-bound record with optional AUTH evidence.
    V2(TransportRecordV2),
}

impl TransportRecordEnvelope {
    /// Wire version of this record.
    pub fn version(&self) -> u32 {
        match self {
            Self::V1(record) => record.version,
            Self::V2(record) => record.version,
        }
    }

    /// Canonical runtime identity carried by this record.
    pub fn runtime_id(&self) -> &str {
        match self {
            Self::V1(record) => &record.runtime_id,
            Self::V2(record) => &record.runtime_id,
        }
    }

    /// Generation nonce carried by this record.
    pub fn start_nonce(&self) -> &str {
        match self {
            Self::V1(record) => &record.start_nonce,
            Self::V2(record) => &record.start_nonce,
        }
    }

    /// Monotonic record sequence.
    pub fn sequence(&self) -> u64 {
        match self {
            Self::V1(record) => record.sequence,
            Self::V2(record) => record.sequence,
        }
    }

    /// Record wall-clock timestamp.
    pub fn timestamp_ms(&self) -> u64 {
        match self {
            Self::V1(record) => record.timestamp_ms,
            Self::V2(record) => record.timestamp_ms,
        }
    }

    /// Whether this record is the terminal startup failure.
    pub fn terminal(&self) -> bool {
        match self {
            Self::V1(record) => record.terminal,
            Self::V2(record) => record.terminal,
        }
    }

    /// Transport health subobject.
    pub fn transport(&self) -> &TransportStatus {
        match self {
            Self::V1(record) => &record.transport,
            Self::V2(record) => &record.transport,
        }
    }

    /// v2 process binding and AUTH evidence, if present.
    pub fn v2(&self) -> Option<&TransportRecordV2> {
        match self {
            Self::V1(_) => None,
            Self::V2(record) => Some(record),
        }
    }
}

/// Decode a strict v1/v2 transport record without permitting cross-version
/// fields. The version discriminator is inspected before serde decodes the
/// corresponding deny-unknown-fields struct.
pub fn decode_transport_record(bytes: &[u8]) -> Result<TransportRecordEnvelope, String> {
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("local transport status exceeds the size limit".into());
    }
    // Parse once with a visitor that rejects duplicate object member names at
    // every nesting depth. `serde_json::from_slice` is deliberately
    // last-wins for duplicate keys, which would make an attacker-controlled
    // record ambiguous before `deny_unknown_fields` gets a chance to run.
    let value =
        parse_strict_json(bytes).map_err(|_| "local transport status malformed".to_string())?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| "local transport status version is invalid".to_string())?;
    match version {
        // Decode the original bytes after using Value only as a discriminator.
        // Deserializing Value and then from_value would collapse duplicate JSON
        // keys before the strict structs could reject them.
        1 => serde_json::from_slice(bytes)
            .map(TransportRecordEnvelope::V1)
            .map_err(|_| "local transport status v1 record is invalid".to_string()),
        2 => serde_json::from_slice(bytes)
            .map(TransportRecordEnvelope::V2)
            .map_err(|_| "local transport status v2 record is invalid".to_string()),
        _ => Err("local transport status version is unsupported".into()),
    }
}

/// Parse JSON while preserving the wire contract's unique-key requirement.
/// The caller still deserializes the original bytes so strict schema checks
/// remain active on the same representation that was received from disk.
fn parse_strict_json(bytes: &[u8]) -> Result<serde_json::Value, String> {
    use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
    use std::collections::HashSet;
    use std::fmt;

    struct StrictValue;

    impl<'de> DeserializeSeed<'de> for StrictValue {
        type Value = serde_json::Value;

        fn deserialize<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            deserializer.deserialize_any(self)
        }
    }

    impl<'de> Visitor<'de> for StrictValue {
        type Value = serde_json::Value;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("valid JSON with unique object keys")
        }

        fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
            Ok(serde_json::Value::Bool(value))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
            Ok(serde_json::Value::Number(value.into()))
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(serde_json::Value::Number(value.into()))
        }

        fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
            serde_json::Number::from_f64(value)
                .map(serde_json::Value::Number)
                .ok_or_else(|| E::custom("non-finite float"))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
            Ok(serde_json::Value::String(value.to_owned()))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
            Ok(serde_json::Value::String(value))
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(serde_json::Value::Null)
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(serde_json::Value::Null)
        }

        fn visit_some<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            deserializer.deserialize_any(self)
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut values = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            while let Some(value) = seq.next_element_seed(StrictValue)? {
                values.push(value);
            }
            Ok(serde_json::Value::Array(values))
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut seen = HashSet::new();
            let mut values = serde_json::Map::new();
            while let Some(key) = map.next_key::<String>()? {
                if !seen.insert(key.clone()) {
                    return Err(serde::de::Error::custom("duplicate object member name"));
                }
                values.insert(key, map.next_value_seed(StrictValue)?);
            }
            Ok(serde_json::Value::Object(values))
        }
    }

    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = StrictValue
        .deserialize(&mut deserializer)
        .map_err(|_| "invalid JSON".to_string())?;
    deserializer
        .end()
        .map_err(|_| "trailing JSON data".to_string())?;
    Ok(value)
}

impl TransportRecordV2 {
    /// Validate v2-only shape and ordering before native identity checks.
    pub fn validate_shape(&self) -> Result<(), String> {
        if self.version != 2
            || self.process_id == 0
            || self.sequence == 0
            || !self.transport.has_safe_error()
            || self.spawn_started_at_ms == 0
            || self.spawn_started_at_ms > self.timestamp_ms
        {
            return Err("local transport v2 record shape is invalid".into());
        }
        let Some(attempt) = &self.connection_attempt else {
            if self.received_auth.is_some() {
                return Err("local transport auth evidence has no connection attempt".into());
            }
            return Ok(());
        };
        if attempt.sequence == 0
            || !is_canonical_uuid(&attempt.id)
            || attempt.started_at_ms == 0
            || attempt.started_at_ms < self.spawn_started_at_ms
            || attempt.started_at_ms > self.timestamp_ms
        {
            return Err("local transport connection attempt is invalid".into());
        }
        let Some(auth) = &self.received_auth else {
            return Ok(());
        };
        if auth.accepted
            || auth.attempt_id != attempt.id
            || auth.attempt_sequence != attempt.sequence
            || !is_lower_hex_64(&auth.auth_event_id)
            || auth.received_at_ms == 0
            || auth.received_at_ms < attempt.started_at_ms
            || auth.received_at_ms > self.timestamp_ms
        {
            return Err("local transport AUTH evidence is invalid".into());
        }
        Ok(())
    }
}

fn is_canonical_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil() && id.to_string() == value)
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::{decode_transport_record, TransportRecordEnvelope, MAX_RECORD_BYTES};

    const V1_JSON: &str = r#"{"version":1,"runtimeId":"fixture-runtime","startNonce":"fixture-nonce","sequence":1,"timestampMs":1000,"terminal":false,"transport":{"state":"connected","code":"none","attempts":0,"elapsedMs":0,"nextRetryAtMs":null,"lastError":null}}"#;
    const V2_JSON: &str = r#"{"version":2,"runtimeId":"fixture-runtime","startNonce":"fixture-nonce","sequence":1,"timestampMs":1000,"terminal":false,"transport":{"state":"connected","code":"none","attempts":0,"elapsedMs":0,"nextRetryAtMs":null,"lastError":null},"processId":42,"spawnStartedAtMs":900,"connectionAttempt":null,"receivedAuth":null}"#;

    fn with_extra_member(document: &str, member: &str) -> Vec<u8> {
        let mut document = document.to_owned();
        let closing = document.rfind('}').expect("fixture has a root object");
        document.insert_str(closing, &format!(",{member}"));
        document.into_bytes()
    }

    #[test]
    fn decode_transport_record_accepts_valid_v1_and_v2_original_bytes() {
        assert!(matches!(
            decode_transport_record(V1_JSON.as_bytes()),
            Ok(TransportRecordEnvelope::V1(record)) if record.version == 1
        ));
        assert!(matches!(
            decode_transport_record(V2_JSON.as_bytes()),
            Ok(TransportRecordEnvelope::V2(record)) if record.version == 2
        ));
    }

    #[test]
    fn decode_transport_record_rejects_duplicate_top_level_key_from_original_bytes() {
        let duplicate = V1_JSON.replacen("\"version\":1,", "\"version\":1,\"version\":1,", 1);
        assert!(decode_transport_record(duplicate.as_bytes()).is_err());
    }

    #[test]
    fn decode_transport_record_rejects_duplicate_nested_key_from_original_bytes() {
        let duplicate = V1_JSON.replacen(
            "\"state\":\"connected\",",
            "\"state\":\"connected\",\"state\":\"connected\",",
            1,
        );
        assert!(decode_transport_record(duplicate.as_bytes()).is_err());
    }

    #[test]
    fn decode_transport_record_rejects_v1_unknown_or_null_v2_fields() {
        for (label, bytes) in [
            (
                "unknown process binding",
                with_extra_member(V1_JSON, "\"processId\":42"),
            ),
            (
                "null connection attempt",
                with_extra_member(V1_JSON, "\"connectionAttempt\":null"),
            ),
        ] {
            assert!(
                decode_transport_record(&bytes).is_err(),
                "v1 accepted {label}"
            );
        }
    }

    #[test]
    fn decode_transport_record_rejects_v2_unknown_field() {
        let bytes = with_extra_member(V2_JSON, "\"unknownField\":true");
        assert!(decode_transport_record(&bytes).is_err());
    }

    #[test]
    fn decode_transport_record_rejects_v2_wrong_version() {
        for version in ["1", "3"] {
            let bytes = V2_JSON.replacen("\"version\":2", &format!("\"version\":{version}"), 1);
            assert!(
                decode_transport_record(bytes.as_bytes()).is_err(),
                "v2-shaped record with version {version} was accepted"
            );
        }
    }

    #[test]
    fn decode_transport_record_rejects_oversize_original_bytes() {
        let mut bytes = V2_JSON.as_bytes().to_vec();
        bytes.resize(MAX_RECORD_BYTES as usize + 1, b' ');
        assert!(decode_transport_record(&bytes).is_err());
    }
}
