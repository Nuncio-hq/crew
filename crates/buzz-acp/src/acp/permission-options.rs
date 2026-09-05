//! Permission choices use ACP kinds, never labels or guessed option identifiers.
pub(super) fn selected_option(options: &serde_json::Value, auto_approve: bool) -> Option<&str> {
    let options = options.as_array()?;
    for kind in ["allow_once", "reject_once", "reject_always"] {
        if kind == "allow_once" && !auto_approve {
            continue;
        }
        if let Some(id) = options
            .iter()
            .filter(|option| option["kind"] == kind)
            .filter_map(|option| option["optionId"].as_str())
            .find(|id| !id.trim().is_empty())
        {
            return Some(id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_grant_is_never_inferred_from_one_time_policy() {
        let options = serde_json::json!([
            {"kind":"allow_always","optionId":"persistent"},
            {"kind":"reject_once","optionId":"no"}
        ]);
        assert_eq!(selected_option(&options, true), Some("no"));
        assert_eq!(
            selected_option(
                &serde_json::json!([{ "kind":"allow_always","optionId":"persistent" }]),
                true
            ),
            None
        );
    }
}

#[cfg(test)]
#[test]
fn non_bypass_mode_never_auto_grants_permission() {
    assert_eq!(
        selected_option(
            &serde_json::json!([
                {"kind":"allow_once","optionId":"once"},
                {"kind":"reject_once","optionId":"no"}
            ]),
            false
        ),
        Some("no")
    );
    assert_eq!(
        selected_option(
            &serde_json::json!([
                {"kind":"allow_once","optionId":"once"}
            ]),
            false
        ),
        None
    );
}
