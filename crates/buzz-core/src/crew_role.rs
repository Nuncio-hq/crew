//! Founder-signed, channel-scoped Crew role assignments.
//!
//! A role block is deliberately a small, forward-compatible YAML document
//! embedded in a channel's founder-authored canvas. The canvas event supplies
//! the channel scope; this module supplies parsing, validation, authority
//! filtering, and prompt framing without maintaining a role taxonomy.

use std::collections::BTreeMap;

use nostr::{FromBech32, PublicKey};
use serde::Deserialize;
use thiserror::Error;

const MAX_LABEL_LEN: usize = 128;

/// A resolved role assignment for one agent in one channel canvas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleAssignment {
    /// Founder-authored display label.
    pub label: String,
    /// Founder-authored allowed/not-allowed/redirect definition.
    pub definition: String,
}

/// Parsed content of a `crew` canvas block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasRoleBlock {
    /// Agent pubkey to founder-authored label.
    pub assignments: BTreeMap<String, String>,
    /// Case-folded role label to definition text.
    pub definitions: BTreeMap<String, String>,
    /// Founder-authored work type to role label presets.
    pub routing: BTreeMap<String, String>,
}

/// A routing preset resolved to agents holding its role in the channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingAssignment {
    /// Founder-authored work type.
    pub work_type: String,
    /// Founder-authored role label.
    pub role_label: String,
    /// Canonical holder pubkeys.
    pub holders: Vec<String>,
}

/// Errors that make a crew block fail closed.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RoleParseError {
    /// The fenced block is not structurally complete.
    #[error("malformed crew fenced block")]
    MalformedFence,
    /// The YAML document does not match the supported shape.
    #[error("invalid crew YAML: {0}")]
    InvalidYaml(String),
    /// A role label violates the format-only constraints.
    #[error("invalid crew role label: {0}")]
    InvalidLabel(String),
    /// A pubkey in the canvas or authority inputs is not a valid Nostr key.
    #[error("invalid crew pubkey: {0}")]
    InvalidPubkey(String),
    /// An assignment points at a role without founder-authored meaning.
    #[error("assignment for role `{0}` has no definition")]
    MissingDefinition(String),
}

#[derive(Debug, Deserialize)]
struct RawCanvasRoleBlock {
    #[serde(default)]
    assignments: BTreeMap<String, String>,
    #[serde(default)]
    definitions: BTreeMap<String, String>,
    #[serde(default)]
    routing: BTreeMap<String, String>,
}

/// Parse the first fenced ` ```crew ` block in canvas content.
///
/// No block is represented by `Ok(None)`. A partial or malformed block is an
/// error so callers can warn and emit no role section rather than panicking.
/// If multiple blocks are present, the first block wins; callers should warn.
pub fn parse_canvas_assignments(
    canvas_content: &str,
) -> Result<Option<CanvasRoleBlock>, RoleParseError> {
    let starts: Vec<usize> = canvas_content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| (line.trim() == "```crew").then_some(index))
        .collect();
    let Some(&start) = starts.first() else {
        return Ok(None);
    };
    let lines: Vec<&str> = canvas_content.lines().collect();
    let Some(end_offset) = lines[start + 1..]
        .iter()
        .position(|line| line.trim() == "```")
    else {
        return Err(RoleParseError::MalformedFence);
    };
    let end = start + 1 + end_offset;
    let yaml = lines[start + 1..end].join("\n");
    let raw: RawCanvasRoleBlock =
        serde_yaml::from_str(&yaml).map_err(|e| RoleParseError::InvalidYaml(e.to_string()))?;
    let mut assignments = BTreeMap::new();
    for (agent, label) in raw.assignments {
        let agent = agent.trim().to_string();
        if agent.is_empty() {
            return Err(RoleParseError::InvalidYaml(
                "assignment agent key must not be empty".into(),
            ));
        }
        let label = normalize_label(&label)?;
        assignments.insert(agent, label);
    }
    let mut definitions = BTreeMap::new();
    for (label, definition) in raw.definitions {
        let normalized = normalize_label(&label)?;
        definitions.insert(normalized.to_ascii_lowercase(), definition);
    }
    let mut routing = BTreeMap::new();
    for (work_type, label) in raw.routing {
        let work_type = normalize_label(&work_type)?;
        let label = normalize_label(&label)?;
        routing.insert(work_type.to_ascii_lowercase(), label);
    }
    Ok(Some(CanvasRoleBlock {
        assignments,
        definitions,
        routing,
    }))
}

/// Resolve founder-authored routing presets to role holders in this channel.
pub fn resolve_routing(
    canvas_content: &str,
    canvas_author: &str,
    owner_pubkey: &str,
) -> Result<Option<Vec<RoutingAssignment>>, RoleParseError> {
    if !same_pubkey(canvas_author, owner_pubkey)? {
        return Ok(None);
    }
    let Some(block) = parse_canvas_assignments(canvas_content)? else {
        return Ok(None);
    };
    if block.routing.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let mut resolved = Vec::with_capacity(block.routing.len());
    for (work_type, role_label) in block.routing {
        let mut holders = Vec::new();
        for (agent, assigned_label) in &block.assignments {
            if assigned_label.eq_ignore_ascii_case(&role_label) {
                holders.push(parse_pubkey(agent)?.to_hex());
            }
        }
        resolved.push(RoutingAssignment {
            work_type,
            role_label,
            holders,
        });
    }
    Ok(Some(resolved))
}

/// Resolve one exact work type without guessing unknown work.
pub fn resolve_routing_work_type<'a>(
    routing: &'a [RoutingAssignment],
    work_type: &str,
) -> Option<&'a RoutingAssignment> {
    let work_type = work_type.trim().to_ascii_lowercase();
    routing.iter().find(|preset| preset.work_type == work_type)
}

/// Compose channel routing presets and explicit founder escalation rows.
pub fn compose_routing_section(routing: &[RoutingAssignment]) -> String {
    if routing.is_empty() {
        return String::new();
    }
    let mut section = String::from("## Channel routing presets (Crew)\n\n");
    for preset in routing {
        section.push_str(&format!(
            "- `{}` → role `{}`: ",
            preset.work_type, preset.role_label
        ));
        if preset.holders.is_empty() {
            section.push_str(&format!(
                "no agent holds `{}` in this channel — ask the founder\n",
                preset.role_label
            ));
        } else {
            section.push_str(&format!("holder(s): {}\n", preset.holders.join(", ")));
        }
    }
    section.push_str(
        "\nUse only exact work-type matches. Unknown work types must not be guessed; ask the founder.\n",
    );
    section
}

/// Count complete or partial ` ```crew ` fence openers in canvas content.
///
/// The resolver intentionally uses the first block when this returns more
/// than one; callers can use the count to emit a warning.
pub fn count_crew_blocks(canvas_content: &str) -> usize {
    canvas_content
        .lines()
        .filter(|line| line.trim() == "```crew")
        .count()
}

/// Remove the first fenced `crew` block while preserving surrounding prose.
///
/// The block is machine configuration and should not be copied into rendered
/// canvas prose when the role/routing sections already carry its meaning.
pub fn strip_crew_block(canvas_content: &str) -> Result<String, RoleParseError> {
    let Some(start) = canvas_content
        .lines()
        .position(|line| line.trim() == "```crew")
    else {
        return Ok(canvas_content.to_string());
    };
    let lines: Vec<&str> = canvas_content.lines().collect();
    let Some(end_offset) = lines[start + 1..]
        .iter()
        .position(|line| line.trim() == "```")
    else {
        return Err(RoleParseError::MalformedFence);
    };
    let end = start + 1 + end_offset;
    let mut kept = lines[..start].to_vec();
    kept.extend_from_slice(&lines[end + 1..]);
    Ok(kept.join("\n"))
}

/// Resolve one agent's assignment after checking canvas author authority.
///
/// The caller must pass the author of the channel canvas event. The canvas
/// write path currently has no relay-side founder check, so this read-side
/// comparison is the enforcement point.
pub fn resolve_assignment(
    canvas_content: &str,
    canvas_author: &str,
    owner_pubkey: &str,
    agent_pubkey: &str,
) -> Result<Option<RoleAssignment>, RoleParseError> {
    if !same_pubkey(canvas_author, owner_pubkey)? {
        return Ok(None);
    }
    let Some(block) = parse_canvas_assignments(canvas_content)? else {
        return Ok(None);
    };
    let mut label = None;
    for (agent, assignment_label) in &block.assignments {
        if same_pubkey(agent, agent_pubkey)? {
            label = Some(assignment_label.clone());
            break;
        }
    }
    let Some(label) = label else {
        return Ok(None);
    };
    let Some(definition) = block.definitions.get(&label.to_ascii_lowercase()) else {
        return Err(RoleParseError::MissingDefinition(label));
    };
    Ok(Some(RoleAssignment {
        label,
        definition: definition.clone(),
    }))
}

/// Compose fixed Crew framing around founder-authored assignment text.
pub fn compose_role_section(assignment: &RoleAssignment) -> String {
    format!(
        "## Role assignment (Crew)\n\n\
         You are assigned role: **{}**.\n\n\
         The founder-authored role definition below is authoritative:\n\
         {}\n\n\
         When a request is outside this definition:\n\
         1. Do not silently execute it.\n\
         2. Refuse briefly and redirect to the appropriate role or founder.\n\
         3. Do not partially perform the off-role work.\n\n\
         MANDATORY declaration: The FIRST line of your first reply message for each turn MUST be exactly:\n\n\
         ROLE-CHECK: role={} decision=accept|refuse reason=<short>\n\n\
         Never omit ROLE-CHECK, including for short answers.",
        assignment.label, assignment.definition, assignment.label
    )
}

fn normalize_label(raw: &str) -> Result<String, RoleParseError> {
    let label = raw.trim();
    if label.is_empty() {
        return Err(RoleParseError::InvalidLabel("label is empty".into()));
    }
    if label.len() > MAX_LABEL_LEN {
        return Err(RoleParseError::InvalidLabel(format!(
            "label exceeds {MAX_LABEL_LEN} bytes"
        )));
    }
    if label.lines().count() != 1 {
        return Err(RoleParseError::InvalidLabel(
            "label must be one line".into(),
        ));
    }
    Ok(label.to_string())
}

fn same_pubkey(left: &str, right: &str) -> Result<bool, RoleParseError> {
    let left = parse_pubkey(left)?;
    let right = parse_pubkey(right)?;
    Ok(left == right)
}

fn parse_pubkey(value: &str) -> Result<PublicKey, RoleParseError> {
    let value = value.trim();
    PublicKey::from_hex(value)
        .or_else(|_| PublicKey::from_bech32(value))
        .map_err(|_| RoleParseError::InvalidPubkey(value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_top_level_keys_are_ignored() {
        let parsed = parse_canvas_assignments(
            "```crew\nassignments:\n  agent: code\ndefinitions:\n  code: text\nrouting: {}\n```",
        )
        .unwrap()
        .unwrap();
        assert_eq!(parsed.assignments["agent"], "code");
    }

    #[test]
    fn malformed_block_is_an_error() {
        assert_eq!(
            parse_canvas_assignments("```crew\nassignments:\n"),
            Err(RoleParseError::MalformedFence)
        );
    }
}
