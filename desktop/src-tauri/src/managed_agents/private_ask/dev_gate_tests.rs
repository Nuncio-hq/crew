//! The gate must be closed in a release build unless the exact value is set.

use super::*;
use std::ffi::OsString;

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

fn value(raw: &str) -> OsString {
    OsString::from(raw)
}

#[test]
fn a_release_build_without_the_variable_keeps_the_surface_closed() {
    assert!(!dev_gate_open(false, None));
}

#[test]
fn a_debug_build_opens_the_surface_without_any_variable() {
    assert!(dev_gate_open(true, None));
}

#[test]
fn only_the_exact_value_opens_a_release_build() {
    assert!(dev_gate_open(false, Some(&value("1"))));
    for refused in [
        "", " ", "0", "1 ", " 1", "true", "TRUE", "yes", "on", "11", "01",
    ] {
        assert!(
            !dev_gate_open(false, Some(&value(refused))),
            "{refused:?} must not open the private Ask surface"
        );
    }
}

/// An environment can hold bytes that are not UTF-8. That is not the enabling
/// value, and reading it must not panic.
#[cfg(unix)]
#[test]
fn a_non_utf8_value_is_refused_rather_than_fatal() {
    let raw = OsString::from_vec(vec![0xff, 0xfe, b'1']);
    assert!(!dev_gate_open(false, Some(&raw)));
}

/// The cached reader agrees with the pure decision for this build.
#[test]
fn the_cached_reader_matches_the_decision_for_this_build() {
    assert_eq!(
        private_ask_dev_enabled(),
        dev_gate_open(
            cfg!(debug_assertions),
            std::env::var_os(PRIVATE_ASK_DEV_VAR).as_deref()
        )
    );
}
