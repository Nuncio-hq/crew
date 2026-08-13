//! Incremental regeneration set from source-file manifests + git diff.

use crate::git_snapshot::{FileChange, FileChangeKind};
use crate::publish::PageDraft;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Which pages to regenerate and whether the TOC should be re-planned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegenPlan {
    /// Page slugs whose source files changed (including rename/move).
    pub slugs: Vec<String>,
    /// Re-run cluster planning (file set changed materially).
    pub replan_toc: bool,
}

/// Pages whose source files intersect the diff. Renames match old or new path.
pub fn regen_plan(pages: &[PageDraft], diff: &[FileChange], replan_toc: bool) -> RegenPlan {
    let mut touched: BTreeSet<String> = BTreeSet::new();
    for change in diff {
        touched.insert(change.path.clone());
        if let Some(old) = &change.old_path {
            touched.insert(old.clone());
        }
        if change.kind == FileChangeKind::Rename {
            if let Some(old) = &change.old_path {
                touched.insert(old.clone());
            }
            touched.insert(change.path.clone());
        }
    }
    let mut slugs: Vec<String> = pages
        .iter()
        .filter(|page| page.source_files.iter().any(|f| touched.contains(f)))
        .map(|page| page.slug.clone())
        .collect();
    slugs.sort();
    slugs.dedup();
    RegenPlan { slugs, replan_toc }
}

/// True when the file set changed enough to re-plan the TOC.
///
/// Material = more than 20% of paths added or removed, or a new top-level
/// directory appeared.
pub fn material_file_set_change(old_files: &[String], new_files: &[String]) -> bool {
    let old: BTreeSet<&str> = old_files.iter().map(String::as_str).collect();
    let new: BTreeSet<&str> = new_files.iter().map(String::as_str).collect();
    if old.is_empty() {
        return !new.is_empty();
    }
    let added = new.difference(&old).count();
    let removed = old.difference(&new).count();
    let denom = old.len().max(1);
    let ratio = (added + removed) as f64 / denom as f64;
    if ratio > 0.20 {
        return true;
    }
    let old_tops: BTreeSet<&str> = old.iter().filter_map(|p| p.split('/').next()).collect();
    let new_tops: BTreeSet<&str> = new.iter().filter_map(|p| p.split('/').next()).collect();
    new_tops.difference(&old_tops).next().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_snapshot::FileChange;

    fn draft(slug: &str, files: &[&str]) -> PageDraft {
        PageDraft {
            slug: slug.into(),
            title: slug.into(),
            section: "overview".into(),
            source_files: files.iter().map(|s| (*s).to_string()).collect(),
            commit: "old".into(),
            language: "en".into(),
            content: String::new(),
        }
    }

    #[test]
    fn rename_selects_page_that_listed_old_path() {
        let pages = vec![
            draft("relay", &["crates/buzz-relay/src/lib.rs"]),
            draft("overview", &["README.md"]),
        ];
        let diff = vec![FileChange {
            path: "crates/buzz-relay/src/server.rs".into(),
            old_path: Some("crates/buzz-relay/src/lib.rs".into()),
            kind: FileChangeKind::Rename,
        }];
        let plan = regen_plan(&pages, &diff, false);
        assert_eq!(plan.slugs, vec!["relay".to_string()]);
        assert!(!plan.replan_toc);
    }

    #[test]
    fn unmodified_pages_are_not_selected() {
        let pages = vec![draft("overview", &["README.md"])];
        let diff = vec![FileChange {
            path: "desktop/src/app/App.tsx".into(),
            old_path: None,
            kind: FileChangeKind::Modify,
        }];
        let plan = regen_plan(&pages, &diff, false);
        assert!(plan.slugs.is_empty());
    }

    #[test]
    fn material_change_on_new_top_level_dir() {
        let old = vec!["README.md".into(), "src/lib.rs".into()];
        let new = vec![
            "README.md".into(),
            "src/lib.rs".into(),
            "mobile/lib/main.dart".into(),
        ];
        assert!(material_file_set_change(&old, &new));
    }
}
