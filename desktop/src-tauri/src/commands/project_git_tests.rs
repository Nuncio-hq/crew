use super::*;

#[test]
fn parse_ls_tree_keeps_paths_after_eager_preview_limit() {
    const EAGER_PREVIEW_LIMIT: usize = 250;

    let repo_dir = tempfile::tempdir().expect("create temporary repository");
    std::fs::create_dir(repo_dir.path().join("src")).expect("create source directory");
    std::fs::write(repo_dir.path().join("README.md"), "# Deferred README")
        .expect("write deferred README");
    std::fs::write(
        repo_dir.path().join("src/application.rs"),
        "fn deferred() {}",
    )
    .expect("write deferred source file");
    let hidden_entries = (0..EAGER_PREVIEW_LIMIT)
        .map(|index| {
            format!(
                "100644 blob {} 1\t.agents/generated-{index:03}.txt",
                "a".repeat(40)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let output = format!(
        "{hidden_entries}\n100644 blob {} 17\tREADME.md\n100644 blob {} 16\tsrc/application.rs",
        "b".repeat(40),
        "c".repeat(40)
    );

    let files = parse_ls_tree(repo_dir.path(), &output, &std::collections::HashMap::new());

    assert_eq!(files.len(), EAGER_PREVIEW_LIMIT + 2);
    assert_eq!(
        files.last().map(|file| file.path.as_str()),
        Some("src/application.rs")
    );
    assert!(files
        .iter()
        .skip(EAGER_PREVIEW_LIMIT)
        .all(|file| file.preview_content.is_none()));
}
