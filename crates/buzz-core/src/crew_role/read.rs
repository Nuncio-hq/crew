//! Display metadata for the native canvas read response; never execution policy.

use super::{parse_canvas_assignments, same_pubkey};
use serde::Serialize;
use std::collections::BTreeMap;

#[cfg(test)]
#[path = "editor_tests.rs"]
mod editor_tests;

/// Additional editable fields on the existing canvas response.
#[derive(Debug, Serialize)]
pub struct CanvasCrewMetadata {
    /// Display definitions, including unassigned roles.
    pub definitions: Vec<DisplayDefinition>,
    /// Canonical contact pubkey, used only as metadata.
    pub contact_pubkey: Option<String>,
    /// Whether the canvas author matches the current owner identity.
    pub crew_authority: &'static str,
    /// Whether the first Crew fence is absent, valid, or invalid.
    pub crew_parse_state: &'static str,
    /// Stored assignment spellings, including unresolved entries; display only.
    pub stored_assignments: BTreeMap<String, String>,
    /// Existing work-type references for explicit deletion resolution.
    pub stored_routing: BTreeMap<String, String>,
    /// Existing capability references for explicit deletion resolution.
    pub stored_capabilities: BTreeMap<String, Vec<String>>,
}

/// A normalized lookup key's retained display label and definition.
#[derive(Debug, Serialize)]
pub struct DisplayDefinition {
    /// Original display label.
    pub role_label: String,
    /// Founder-authored meaning.
    pub definition: String,
}

/// Read display metadata independently of role-execution authority. Foreign
/// content remains visible for explicit review and is always marked foreign.
pub fn read_canvas_crew_metadata(
    content: Option<&str>,
    author: Option<&str>,
    owner: &str,
) -> CanvasCrewMetadata {
    let mut metadata = CanvasCrewMetadata {
        definitions: Vec::new(),
        contact_pubkey: None,
        crew_authority: "absent",
        crew_parse_state: "absent",
        stored_assignments: BTreeMap::new(),
        stored_routing: BTreeMap::new(),
        stored_capabilities: BTreeMap::new(),
    };
    let Some(content) = content else {
        return metadata;
    };
    metadata.crew_authority = if author.is_some_and(|author| same_pubkey(author, owner) == Ok(true))
    {
        "owner"
    } else {
        "foreign"
    };
    match parse_canvas_assignments(content) {
        Ok(Some(block)) => {
            metadata.crew_parse_state = "valid";
            metadata.contact_pubkey = block.contact_pubkey;
            // Execution parsing trims keys/labels. Editor preservation must use
            // original spellings, including distinct trim-equivalent entries.
            let raw = super::bulk::split_fence(content).and_then(|parts| match parts {
                Some((_, yaml, _)) => super::document::parse_document(yaml).map(Some),
                None => Ok(None),
            });
            match raw {
                Ok(Some(document)) => {
                    if let Some(mapping) = document.as_mapping() {
                        metadata.stored_assignments = super::document::assignment_entries(mapping);
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    metadata.crew_parse_state = "invalid";
                    return metadata;
                }
            }
            metadata.stored_routing = block.routing;
            metadata.stored_capabilities = block.capabilities;
            metadata.definitions = block
                .definitions
                .into_iter()
                .map(|(key, definition)| DisplayDefinition {
                    role_label: block.definition_labels.get(&key).cloned().unwrap_or(key),
                    definition,
                })
                .collect();
        }
        Ok(None) => {}
        Err(_) => metadata.crew_parse_state = "invalid",
    }
    metadata
}
