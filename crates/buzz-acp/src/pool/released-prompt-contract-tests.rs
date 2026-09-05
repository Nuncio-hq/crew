use super::*;

#[test]
fn delivery_diagnostic_orders_event_ids_deterministically() {
    let channel = Uuid::new_v4();
    assert_eq!(
        delivery_receipt_line(channel, &HashSet::from(["b".into(), "a".into()])),
        format!("turn delivered Buzz events for channel {channel}: a,b")
    );
}

#[test]
fn checkout_notice_stays_inside_workspace_between_base_and_persona() {
    let notice = "Retained existing checkout. Do not reset its uncommitted changes.";
    let prompt = framed_system_prompt_with_notice(
        "/work/tree",
        Some("static base"),
        Some("persona"),
        Some(notice),
    )
    .expect("standing prompt");
    assert_eq!(prompt,
        format!("<base>\nstatic base\n</base>\n\n<workspace>\nCurrent working directory: /work/tree\n\n[Checkout]\n{notice}\n</workspace>\n\n<system>\npersona\n</system>"));
}
