//! Pure canvas edits shared by bulk save and member cleanup.

use super::{
    document::parse_document, normalize_label, parse_canvas_assignments, parse_pubkey,
    RoleParseError,
};
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};
use std::collections::BTreeMap;

/// One editable role, including roles with no assigned agent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrewRoleDefinition {
    /// Display label; lookup retains existing ASCII case normalization.
    pub label: String,
    /// Founder-authored role meaning.
    pub definition: String,
}

/// One complete role form, applied to the current authoritative canvas.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CrewConfigDraft {
    /// All desired definitions, including unassigned roles.
    pub definitions: Vec<CrewRoleDefinition>,
    /// Canonical pubkey to display role label; at most one role per agent.
    pub assignments: BTreeMap<String, String>,
    /// Optional contact metadata; this never enables routing.
    pub contact: Option<String>,
    /// Original label to replacement label for preserving exact role references.
    #[serde(default)]
    pub renames: BTreeMap<String, String>,
}

/// Clear departing members' assignments/contact through the same bulk edit
/// contract, retaining role definitions and all unrelated canvas values.
/// Unresolved legacy entries are retained. No fence or no matching members is
/// an unchanged no-op (the size cap applies only when reconstructing a canvas).
pub fn remove_canvas_crew_members(
    content: &str,
    members: &[String],
    max_content_bytes: usize,
) -> Result<String, RoleParseError> {
    let Some(block) = parse_canvas_assignments(content)? else {
        return Ok(content.to_string());
    };
    let members = members
        .iter()
        .map(|member| parse_pubkey(member).map(|key| key.to_hex()))
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
    let Some((_, yaml, _)) = split_fence(content)? else {
        return Ok(content.to_string());
    };
    let mut document = parse_document(yaml)?;
    let root = document
        .as_mapping_mut()
        .ok_or_else(|| invalid("crew YAML must be a mapping"))?;
    let mut changed = false;
    if let Some(assignments) = root
        .get_mut(Value::String("assignments".into()))
        .and_then(Value::as_mapping_mut)
    {
        let before = assignments.len();
        assignments.retain(|key, _| {
            !key.as_str()
                .and_then(|key| parse_pubkey(key).ok())
                .is_some_and(|key| members.contains(&key.to_hex()))
        });
        changed |= assignments.len() != before;
    }
    if block
        .contact_pubkey
        .is_some_and(|key| members.contains(&key))
    {
        root.remove(Value::String("contact".into()));
        changed = true;
    }
    if !changed {
        return Ok(content.to_string());
    }
    serialize_canvas(content, root, max_content_bytes)
}

/// Reconstruct the first Crew fence while preserving other semantic values
/// and every byte outside that fence. The caller supplies its event content cap.
pub fn update_canvas_crew_config(
    content: &str,
    draft: &CrewConfigDraft,
    max_content_bytes: usize,
) -> Result<String, RoleParseError> {
    if content.len() > max_content_bytes {
        return Err(invalid("canvas exceeds event content limit"));
    }
    let old = parse_canvas_assignments(content)?;
    let parts = split_fence(content)?;
    let mut root = match parts {
        Some((_, yaml, _)) => parse_document(yaml)?
            .as_mapping()
            .cloned()
            .ok_or_else(|| invalid("crew YAML must be a mapping"))?,
        None => Mapping::new(),
    };
    let mut definitions = Mapping::new();
    let mut labels = BTreeMap::new();
    for definition in &draft.definitions {
        let label = normalize_label(&definition.label)?;
        if labels
            .insert(label.to_ascii_lowercase(), label.clone())
            .is_some()
        {
            return Err(invalid("duplicate role label"));
        }
        definitions.insert(
            Value::String(label),
            Value::String(definition.definition.clone()),
        );
    }
    let mut renames = BTreeMap::new();
    for (from, to) in &draft.renames {
        let from = normalize_label(from)?.to_ascii_lowercase();
        let to = normalize_label(to)?.to_ascii_lowercase();
        let old = old
            .as_ref()
            .ok_or_else(|| invalid("rename requires an existing role"))?;
        if !old.definitions.contains_key(&from)
            || !labels.contains_key(&to)
            || (from != to && old.definitions.contains_key(&to))
            || (from != to && labels.contains_key(&from))
            || renames.values().any(|existing| existing == &to)
            || renames.insert(from, to).is_some()
        {
            return Err(invalid("invalid or colliding role rename"));
        }
    }
    let mut assignments = Mapping::new();
    for (agent, label) in &draft.assignments {
        let agent = parse_pubkey(agent)?.to_hex();
        let label = normalize_label(label)?;
        let canonical = label.to_ascii_lowercase();
        let target = renames.get(&canonical).unwrap_or(&canonical);
        let label = labels
            .get(target)
            .ok_or_else(|| RoleParseError::MissingDefinition(label.clone()))?;
        if assignments
            .insert(Value::String(agent), Value::String(label.clone()))
            .is_some()
        {
            return Err(invalid("duplicate canonical agent key"));
        }
    }
    // Preserve unknown values and rewrite only exact role references.
    for field in ["routing", "capabilities"] {
        if let Some(value) = root.get_mut(Value::String(field.into())) {
            let mapping = value
                .as_mapping()
                .ok_or_else(|| invalid("role references must be mappings"))?;
            let mut updated = Mapping::new();
            for (key, value) in mapping {
                let reference = if field == "routing" { value } else { key };
                let label = normalize_label(
                    reference
                        .as_str()
                        .ok_or_else(|| invalid("role reference must be text"))?,
                )?;
                let canonical = label.to_ascii_lowercase();
                let target = renames.get(&canonical).unwrap_or(&canonical);
                if !labels.contains_key(target)
                    && old
                        .as_ref()
                        .is_some_and(|block| block.definitions.contains_key(&canonical))
                {
                    return Err(RoleParseError::MissingDefinition(label));
                }
                let replacement = if renames.contains_key(&canonical) {
                    Value::String(labels[target].clone())
                } else {
                    reference.clone()
                };
                let (key, value) = if field == "routing" {
                    (key.clone(), replacement)
                } else {
                    (replacement, value.clone())
                };
                if updated.insert(key, value).is_some() {
                    return Err(invalid("colliding role reference"));
                }
            }
            *value = Value::Mapping(updated);
        }
    }
    root.insert(
        Value::String("definitions".into()),
        Value::Mapping(definitions),
    );
    root.insert(
        Value::String("assignments".into()),
        Value::Mapping(assignments),
    );
    let contact_key = Value::String("contact".into());
    if let Some(contact) = &draft.contact {
        root.insert(contact_key, Value::String(parse_pubkey(contact)?.to_hex()));
    } else {
        root.remove(&contact_key);
    }
    serialize_canvas(content, &root, max_content_bytes)
}

fn serialize_canvas(
    content: &str,
    root: &Mapping,
    max_content_bytes: usize,
) -> Result<String, RoleParseError> {
    if content.len() > max_content_bytes {
        return Err(invalid("canvas exceeds event content limit"));
    }
    let yaml = serde_yaml::to_string(root).map_err(|e| invalid(&e.to_string()))?;
    let updated = match split_fence(content)? {
        Some((prefix, _, suffix)) => format!("{prefix}```crew\n{yaml}```{suffix}"),
        None => format!(
            "{content}{}```crew\n{yaml}```",
            if content.is_empty() || content.ends_with('\n') {
                ""
            } else {
                "\n"
            }
        ),
    };
    if updated.len() > max_content_bytes {
        return Err(invalid("canvas exceeds event content limit"));
    }
    parse_canvas_assignments(&updated)?;
    Ok(updated)
}

fn invalid(message: &str) -> RoleParseError {
    RoleParseError::InvalidYaml(message.into())
}

/// Borrow byte-exact prose around the same line-delimited first Crew fence.
pub(super) fn split_fence(content: &str) -> Result<Option<(&str, &str, &str)>, RoleParseError> {
    let mut offset = 0;
    let mut opener = None;
    for line in content.split_inclusive('\n') {
        if let Some((start, body)) = opener {
            if line.trim() == "```" {
                let end = offset + line.trim_end_matches(['\r', '\n']).len();
                return Ok(Some((
                    &content[..start],
                    &content[body..offset],
                    &content[end..],
                )));
            }
        } else if line.trim() == "```crew" {
            opener = Some((offset, offset + line.len()));
        }
        offset += line.len();
    }
    if opener.is_some() {
        Err(RoleParseError::MalformedFence)
    } else {
        Ok(None)
    }
}
