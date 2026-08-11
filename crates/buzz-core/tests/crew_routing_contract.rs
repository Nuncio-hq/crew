use buzz_core::crew_role::{compose_routing_section, resolve_routing, resolve_routing_work_type};

#[test]
fn routing_resolves_holders_and_unheld_roles_loudly() {
    let owner = "11".repeat(32);
    let agent = "22".repeat(32);
    let canvas = format!(
        "```crew\nassignments:\n  \"{agent}\": backend-dev\nrouting:\n  backend: backend-dev\n  test: reviewer\ndefinitions:\n  backend-dev: Build services.\n```"
    );
    let routing = resolve_routing(&canvas, &owner, &owner)
        .expect("valid routing")
        .expect("routing exists");
    assert_eq!(routing[0].holders, vec![agent]);
    let section = compose_routing_section(&routing);
    assert!(section.contains("no agent holds `reviewer` in this channel"));
}

#[test]
fn unknown_work_type_is_not_guessed() {
    let routing = resolve_routing(
        "```crew\nrouting:\n  backend: backend-dev\ndefinitions: {}\n```",
        &"11".repeat(32),
        &"11".repeat(32),
    )
    .expect("valid routing")
    .expect("routing exists");
    assert!(resolve_routing_work_type(&routing, "frontend").is_none());
}

#[test]
fn non_founder_canvas_ignores_routing_and_missing_routing_is_unchanged() {
    let owner = "11".repeat(32);
    let other = "22".repeat(32);
    assert_eq!(
        resolve_routing(
            "```crew\nrouting:\n  backend: backend-dev\ndefinitions: {}\n```",
            &other,
            &owner,
        )
        .expect("authority check"),
        None
    );
    assert_eq!(
        resolve_routing("Founder prose.", &owner, &owner).expect("no block"),
        None
    );
}

#[test]
fn malformed_assignment_key_does_not_hide_valid_holder() {
    let owner = "11".repeat(32);
    let valid = "22".repeat(32);
    let canvas = format!(
        "```crew\nassignments:\n  garbage-key: backend\n  \"{valid}\": backend\ndefinitions:\n  backend: Build services.\nrouting:\n  backend: backend\n```"
    );
    let routing = resolve_routing(&canvas, &owner, &owner)
        .expect("valid holder survives")
        .expect("routing present");
    assert_eq!(routing[0].holders, vec![valid]);
}

#[test]
fn routing_holders_render_as_npub_but_retain_hex_values() {
    let owner = "11".repeat(32);
    let holder = "22".repeat(32);
    let canvas = format!(
        "```crew\nassignments:\n  \"{holder}\": backend\ndefinitions:\n  backend: Build services.\nrouting:\n  backend: backend\n```"
    );
    let routing = resolve_routing(&canvas, &owner, &owner)
        .expect("routing")
        .expect("routing present");
    assert_eq!(routing[0].holders, vec![holder.clone()]);
    let prompt = compose_routing_section(&routing);
    assert!(prompt.contains("npub1"));
    assert!(!prompt.contains(&holder));
}
