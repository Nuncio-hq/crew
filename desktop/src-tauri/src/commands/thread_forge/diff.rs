use std::path::Path;

use super::types::{ForgeDiff, ForgeDiffFile, ForgeDiffSource};

const MAX_PATCH_LINES: usize = 2_000;
const MAX_FILES: usize = 250;

/// Choose worktree git when the checkout exists; otherwise API.
pub fn select_diff_source(worktree_path: Option<&str>) -> ForgeDiffSource {
    match worktree_path {
        Some(path) if Path::new(path).is_dir() => ForgeDiffSource::Worktree,
        _ => ForgeDiffSource::Api,
    }
}

pub fn parse_numstat_line(line: &str) -> Option<(String, u64, u64)> {
    let mut parts = line.split('\t');
    let additions = parse_count(parts.next()?);
    let deletions = parse_count(parts.next()?);
    let path = parts.next()?.trim();
    if path.is_empty() {
        return None;
    }
    Some((normalize_numstat_path(path), additions, deletions))
}

/// `old => new` / `{old => new}` become the destination path.
pub fn normalize_numstat_path(path: &str) -> String {
    if let Some((left, right)) = path.split_once(" => ") {
        let right = right.trim();
        if let Some(prefix) = left.find('{') {
            let stem = &left[..prefix];
            return format!("{stem}{right}").replace('}', "");
        }
        return right.to_string();
    }
    path.to_string()
}

fn parse_count(value: &str) -> u64 {
    if value == "-" {
        0
    } else {
        value.parse().unwrap_or(0)
    }
}

pub fn parse_unified_diff(patch: &str) -> Vec<ForgeDiffFile> {
    let mut files = Vec::new();
    let mut current_path = String::new();
    let mut current_patch = String::new();
    let mut additions = 0_u64;
    let mut deletions = 0_u64;
    for line in patch.lines() {
        if let Some(header) = line.strip_prefix("diff --git ") {
            flush_file(
                &mut files,
                &mut current_path,
                &mut current_patch,
                &mut additions,
                &mut deletions,
            );
            current_path = path_from_git_header(header);
            current_patch = format!("{line}\n");
            continue;
        }
        if current_path.is_empty() {
            continue;
        }
        current_patch.push_str(line);
        current_patch.push('\n');
        if let Some(stripped) = line.strip_prefix('+') {
            if !stripped.starts_with("++") {
                additions += 1;
            }
        } else if let Some(stripped) = line.strip_prefix('-') {
            if !stripped.starts_with("--") {
                deletions += 1;
            }
        }
    }
    flush_file(
        &mut files,
        &mut current_path,
        &mut current_patch,
        &mut additions,
        &mut deletions,
    );
    files.truncate(MAX_FILES);
    files
}

fn path_from_git_header(header: &str) -> String {
    // `a/old.txt b/new.txt` — prefer the destination path.
    let mut parts = header.split_whitespace();
    let _old = parts.next();
    let new = parts.next().unwrap_or("");
    new.trim_start_matches("b/").to_string()
}

fn flush_file(
    files: &mut Vec<ForgeDiffFile>,
    path: &mut String,
    patch: &mut String,
    additions: &mut u64,
    deletions: &mut u64,
) {
    if path.is_empty() {
        return;
    }
    let (patch_text, truncated) = truncate_patch(std::mem::take(patch));
    files.push(ForgeDiffFile {
        path: std::mem::take(path),
        additions: *additions,
        deletions: *deletions,
        patch: patch_text,
        truncated,
    });
    *additions = 0;
    *deletions = 0;
}

fn truncate_patch(patch: String) -> (String, bool) {
    let mut line_starts = patch
        .char_indices()
        .filter(|(_, c)| *c == '\n')
        .map(|(index, _)| index);
    match line_starts.nth(MAX_PATCH_LINES - 1) {
        Some(cut_at) => (patch[..cut_at].to_string(), true),
        None => (patch, false),
    }
}

pub fn diff_from_files(files: Vec<ForgeDiffFile>, source: ForgeDiffSource) -> ForgeDiff {
    let additions = files.iter().map(|file| file.additions).sum();
    let deletions = files.iter().map(|file| file.deletions).sum();
    ForgeDiff {
        files,
        additions,
        deletions,
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_numstat_uses_destination_path() {
        let (path, add, del) = parse_numstat_line("0\t0\told.txt => new.txt").expect("line");
        assert_eq!(path, "new.txt");
        assert_eq!((add, del), (0, 0));
        assert_eq!(
            normalize_numstat_path("src/{old.rs => new.rs}"),
            "src/new.rs"
        );
    }

    #[test]
    fn binary_numstat_is_zero_counts() {
        let (path, add, del) = parse_numstat_line("-\t-\tblob.bin").expect("bin");
        assert_eq!(path, "blob.bin");
        assert_eq!((add, del), (0, 0));
    }

    #[test]
    fn unified_diff_splits_files_and_counts() {
        let patch = "\
diff --git a/old.txt b/new.txt
similarity index 100%
rename from old.txt
rename to new.txt
diff --git a/blob.bin b/blob.bin
Binary files a/blob.bin and b/blob.bin differ
diff --git a/added.txt b/added.txt
--- /dev/null
+++ b/added.txt
@@ -0,0 +1 @@
+extra
";
        let files = parse_unified_diff(patch);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "new.txt");
        assert_eq!(files[1].path, "blob.bin");
        assert_eq!(files[2].path, "added.txt");
        assert_eq!(files[2].additions, 1);
    }

    #[test]
    fn missing_worktree_selects_api() {
        assert_eq!(select_diff_source(None), ForgeDiffSource::Api);
        assert_eq!(
            select_diff_source(Some("/definitely-not-a-worktree-xyz")),
            ForgeDiffSource::Api
        );
    }
}
