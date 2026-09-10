//! Unsupported hosts must not silently gain an uncontained source reader.

#[cfg(not(unix))]
#[test]
fn source_capture_fails_closed_on_unsupported_hosts() {
    let root = std::env::temp_dir();
    let folder_error = crew_wiki::source_folder::capture_folder(
        &root,
        &format!("30617:{}:fixture", "a".repeat(64)),
    )
    .expect_err("unsupported folder reader must reject before I/O");
    assert!(folder_error
        .to_string()
        .contains("unavailable on this host"));
    let git_error = crew_wiki::source_snapshot::capture_git(
        &root,
        crew_wiki::source_snapshot::GitSourceChoice::CurrentHead,
    )
    .expect_err("unsupported Git reader must reject before I/O");
    assert!(git_error.to_string().contains("unavailable on this host"));
}
