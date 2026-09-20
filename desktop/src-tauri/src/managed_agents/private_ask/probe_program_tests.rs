//! The marker parser is the seam between a probe's self-report and a receipt.
//!
//! It is tested on its own because every "silence is not success" rule lives
//! here: no marker, a wrong nonce, a truncated object and non-UTF-8 output must
//! each produce no evidence rather than a partially-populated one.

use super::*;

fn marker_line(nonce: &str) -> String {
    format!(
        "{PROBE_MARKER}{{\"nonce\":\"{nonce}\",\"parentPid\":4321,\"writeOutsideDenied\":true,\
         \"linkOutsideDenied\":true,\"readOutsideDenied\":true,\
         \"directConnectDenied\":true,\"ipv6DirectDenied\":true,\
         \"unixConnectDenied\":true,\
         \"foreignConnect\":\"refused\",\"providerConnect\":\"accepted\",\
         \"dnsDenied\":true,\"forkDenied\":true}}\n"
    )
}

#[test]
fn a_complete_marker_with_this_runs_nonce_parses() {
    let parsed = parse_marker(marker_line("nonce-a").as_bytes(), "nonce-a").expect("marker");
    // The lineage is read from the child's own report, so it must survive the
    // parse rather than being filled in by the desktop.
    assert_eq!(parsed.parent_pid, 4321);
    assert!(parsed.write_outside_denied);
    assert!(parsed.link_outside_denied);
    assert!(parsed.read_outside_denied);
    assert!(parsed.direct_connect_denied);
    assert!(parsed.ipv6_direct_denied);
    assert!(parsed.unix_connect_denied);
    assert!(parsed.foreign_connect_refused);
    assert!(parsed.provider_connect_reached_proxy);
    assert!(parsed.dns_denied);
    assert!(parsed.fork_denied);
}

#[test]
fn a_marker_from_another_run_is_not_evidence_about_this_one() {
    assert_eq!(
        parse_marker(marker_line("nonce-a").as_bytes(), "nonce-b"),
        None
    );
}

#[test]
fn an_empty_expected_nonce_never_matches() {
    // Defence against a caller that failed to generate a nonce: an ageless,
    // unbound marker is exactly what the nonce exists to refuse.
    assert_eq!(parse_marker(marker_line("").as_bytes(), ""), None);
}

#[test]
fn silence_is_not_success() {
    assert_eq!(parse_marker(b"", "nonce-a"), None);
    assert_eq!(parse_marker(b"probe ran fine\n", "nonce-a"), None);
}

#[test]
fn a_truncated_marker_is_not_a_partial_result() {
    let line = format!("{PROBE_MARKER}{{\"nonce\":\"nonce-a\",\"writeOutsideDenied\":true");
    assert_eq!(parse_marker(line.as_bytes(), "nonce-a"), None);
}

#[test]
fn a_marker_missing_one_field_yields_nothing_rather_than_a_default() {
    let line = format!(
        "{PROBE_MARKER}{{\"nonce\":\"nonce-a\",\"writeOutsideDenied\":true,\
         \"linkOutsideDenied\":true,\"readOutsideDenied\":true,\
         \"directConnectDenied\":true,\"ipv6DirectDenied\":true,\
         \"unixConnectDenied\":true,\
         \"foreignConnect\":\"refused\",\"providerConnect\":\"accepted\",\
         \"dnsDenied\":true}}"
    );
    assert_eq!(parse_marker(line.as_bytes(), "nonce-a"), None);
}

#[test]
fn non_utf8_output_is_refused_rather_than_lossily_decoded() {
    let mut bytes = marker_line("nonce-a").into_bytes();
    bytes.insert(0, 0xff);
    assert_eq!(parse_marker(&bytes, "nonce-a"), None);
}

#[test]
fn a_foreign_target_the_proxy_accepted_is_not_a_refusal() {
    let line = marker_line("nonce-a").replace("\"refused\"", "\"accepted\"");
    let parsed = parse_marker(line.as_bytes(), "nonce-a").expect("marker");
    assert!(!parsed.foreign_connect_refused);
}

#[test]
fn a_provider_leg_that_never_reached_the_proxy_is_not_evidence() {
    let line = marker_line("nonce-a").replace(
        "\"providerConnect\":\"accepted\"",
        "\"providerConnect\":\"unreachable\"",
    );
    let parsed = parse_marker(line.as_bytes(), "nonce-a").expect("marker");
    assert!(!parsed.provider_connect_reached_proxy);
}

#[test]
fn the_shipped_probe_program_digest_is_stable_for_one_build() {
    assert_eq!(probe_program_digest(), probe_program_digest());
    assert_eq!(probe_program_digest().len(), 64);
}

#[test]
fn the_probe_program_runs_under_the_shipped_interpreter() {
    // The design depends on this interpreter being inside the policy's fixed
    // read allow-list. If it moves, the zero-policy-delta argument moves too.
    assert!(std::path::Path::new(PROBE_INTERPRETER).exists());
}

#[test]
fn a_marker_with_no_parent_is_not_a_lineage() {
    // An absent or zero parent cannot answer "who started this child", and a
    // default would let `independent_invocation` be projected from nothing.
    let line = marker_line("nonce-a").replace("\"parentPid\":4321,", "");
    assert_eq!(parse_marker(line.as_bytes(), "nonce-a"), None);
    let zero = marker_line("nonce-a").replace("\"parentPid\":4321", "\"parentPid\":0");
    assert_eq!(parse_marker(zero.as_bytes(), "nonce-a"), None);
}
