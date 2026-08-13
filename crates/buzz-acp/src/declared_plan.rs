//! Latest ACP `sessionUpdate: plan` snapshot retained on the client (#190).
//!
//! Authority is the structured `entries[]` array only. Missing `entries` is
//! not a snapshot (do not scrape markdown `content`). An explicit empty
//! `entries` list clears the retained snapshot so a later UI projection
//! cannot keep stale rows.

use serde_json::Value;

/// ACP plan entry status as declared by the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclaredPlanStatus {
    Pending,
    InProgress,
    Completed,
}

/// One row from a native ACP plan snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredPlanEntry {
    pub content: String,
    pub status: DeclaredPlanStatus,
}

/// Latest full-replacement plan snapshot for the current ACP session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredPlanSnapshot {
    pub entries: Vec<DeclaredPlanEntry>,
    pub session_id: Option<String>,
}

/// Parse a `sessionUpdate: plan` object.
///
/// Returns `Some` when `entries` is present (including `[]`). Returns `None`
/// when `entries` is absent so unstructured `content` text is not treated as
/// declared tasks.
pub fn parse_acp_plan_update(
    update: &Value,
    session_id: Option<&str>,
) -> Option<DeclaredPlanSnapshot> {
    let entries_value = update.get("entries")?;
    let raw_entries = entries_value.as_array()?;
    let mut entries = Vec::new();
    for entry in raw_entries {
        let Some(content) = entry
            .get("content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
        else {
            continue;
        };
        entries.push(DeclaredPlanEntry {
            content: content.to_owned(),
            status: parse_status(entry.get("status").and_then(Value::as_str)),
        });
    }
    Some(DeclaredPlanSnapshot {
        entries,
        session_id: session_id.map(str::to_owned),
    })
}

/// Retain, replace, or clear the client's latest plan from a plan update.
///
/// `entries: []` clears. Unstructured updates (no `entries` array) leave the
/// previous snapshot untouched.
pub fn apply_plan_update(
    current: &mut Option<DeclaredPlanSnapshot>,
    update: &Value,
    session_id: Option<&str>,
) {
    let Some(snapshot) = parse_acp_plan_update(update, session_id) else {
        return;
    };
    if snapshot.entries.is_empty() {
        *current = None;
        return;
    }
    *current = Some(snapshot);
}

fn parse_status(raw: Option<&str>) -> DeclaredPlanStatus {
    match raw.map(str::trim).unwrap_or("") {
        "in_progress" => DeclaredPlanStatus::InProgress,
        "completed" => DeclaredPlanStatus::Completed,
        _ => DeclaredPlanStatus::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_standard_acp_entries_including_in_progress() {
        let update = json!({
            "sessionUpdate": "plan",
            "entries": [
                {"content": "Fetch Buzz 0.5.11 tags", "status": "completed", "priority": "medium"},
                {"content": "Compare ACP lifecycle", "status": "in_progress"},
                {"content": "Write sync issue", "status": "pending"}
            ]
        });
        let snapshot = parse_acp_plan_update(&update, Some("sess-hermes-dev")).expect("entries");
        assert_eq!(snapshot.session_id.as_deref(), Some("sess-hermes-dev"));
        assert_eq!(snapshot.entries.len(), 3);
        assert_eq!(snapshot.entries[0].status, DeclaredPlanStatus::Completed);
        assert_eq!(snapshot.entries[1].status, DeclaredPlanStatus::InProgress);
        assert_eq!(snapshot.entries[2].content, "Write sync issue");
        assert_eq!(snapshot.entries[2].status, DeclaredPlanStatus::Pending);
    }

    #[test]
    fn cancelled_completed_prefix_is_kept_as_text() {
        let update = json!({
            "entries": [
                {"content": "[cancelled] Old step", "status": "completed"}
            ]
        });
        let snapshot = parse_acp_plan_update(&update, None).expect("entries");
        assert_eq!(snapshot.entries[0].content, "[cancelled] Old step");
        assert_eq!(snapshot.entries[0].status, DeclaredPlanStatus::Completed);
    }

    #[test]
    fn missing_entries_is_not_a_snapshot() {
        let update = json!({
            "sessionUpdate": "plan",
            "content": {"type": "text", "text": "- [ ] Guessed from markdown"}
        });
        assert!(parse_acp_plan_update(&update, None).is_none());
    }

    #[test]
    fn empty_entries_parse_as_empty_snapshot() {
        let update = json!({"sessionUpdate": "plan", "entries": []});
        let snapshot = parse_acp_plan_update(&update, Some("sess")).expect("empty entries");
        assert!(snapshot.entries.is_empty());
    }

    #[test]
    fn apply_replaces_wholesale_including_removals() {
        let mut current = None;
        apply_plan_update(
            &mut current,
            &json!({
                "entries": [
                    {"content": "Keep me", "status": "pending"},
                    {"content": "Drop me later", "status": "pending"}
                ]
            }),
            Some("sess-1"),
        );
        apply_plan_update(
            &mut current,
            &json!({
                "entries": [
                    {"content": "Keep me", "status": "completed"}
                ]
            }),
            Some("sess-1"),
        );
        let snapshot = current.expect("replaced");
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].content, "Keep me");
        assert_eq!(snapshot.entries[0].status, DeclaredPlanStatus::Completed);
    }

    #[test]
    fn apply_empty_entries_clears() {
        let mut current = None;
        apply_plan_update(
            &mut current,
            &json!({"entries": [{"content": "Stale", "status": "pending"}]}),
            Some("sess-1"),
        );
        assert!(current.is_some());
        apply_plan_update(&mut current, &json!({"entries": []}), Some("sess-1"));
        assert!(current.is_none());
    }

    #[test]
    fn apply_unstructured_content_does_not_clobber() {
        let mut current = None;
        apply_plan_update(
            &mut current,
            &json!({"entries": [{"content": "Declared", "status": "pending"}]}),
            Some("sess-1"),
        );
        apply_plan_update(
            &mut current,
            &json!({
                "sessionUpdate": "plan",
                "content": {"type": "text", "text": "- [ ] Markdown"}
            }),
            Some("sess-1"),
        );
        assert_eq!(current.as_ref().unwrap().entries[0].content, "Declared");
    }
}
