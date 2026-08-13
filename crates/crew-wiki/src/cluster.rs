//! Cluster-based page planning. Deterministic without steering.

use crate::git_snapshot::RepoSnapshot;
use crate::steering::WikiSteering;
use crate::types::{PlannedPage, PlannedSection, WikiPlan};
use crate::WikiError;
use std::collections::{BTreeMap, BTreeSet};

const SKIP_DIRS: &[&str] = &[
    "target",
    "node_modules",
    "dist",
    "build",
    ".git",
    "vendor",
    ".hermit",
    "Pods",
    ".dart_tool",
];
const SOURCE_EXTS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "md", "sql", "dart", "py", "go", "toml", "yml", "yaml", "json",
    "css", "html",
];

/// Plan a TOC from a repo snapshot. Steering `pages` bypasses clustering.
pub fn plan_pages(
    snapshot: &RepoSnapshot,
    steering: Option<&WikiSteering>,
) -> Result<WikiPlan, WikiError> {
    let language = steering
        .and_then(|s| s.language.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or("en")
        .to_string();

    if let Some(pages) = steering.and_then(|s| s.pages.as_ref()) {
        if !pages.is_empty() {
            return Ok(WikiPlan {
                language,
                sections: steered_sections(pages, snapshot),
            });
        }
    }

    let files = filter_source_files(&snapshot.files);
    Ok(WikiPlan {
        language,
        sections: cluster_sections(&files, snapshot),
    })
}

fn steered_sections(
    pages: &[crate::steering::SteeringPage],
    snapshot: &RepoSnapshot,
) -> Vec<PlannedSection> {
    let mut by_section: BTreeMap<String, PlannedSection> = BTreeMap::new();
    for (idx, page) in pages.iter().enumerate() {
        let section_id = page
            .section
            .clone()
            .unwrap_or_else(|| "overview".to_string());
        let title = page
            .title
            .clone()
            .unwrap_or_else(|| title_from_slug(&page.slug));
        let source_files = page.source_files.clone().unwrap_or_else(|| {
            snapshot
                .files
                .iter()
                .filter(|f| f.starts_with(&page.slug) || idx == 0)
                .take(24)
                .cloned()
                .collect()
        });
        let planned = PlannedPage {
            slug: slugify(&page.slug),
            title,
            section: section_id.clone(),
            source_files,
        };
        by_section
            .entry(section_id.clone())
            .or_insert_with(|| PlannedSection {
                id: section_id.clone(),
                title: title_from_slug(&section_id),
                pages: Vec::new(),
            })
            .pages
            .push(planned);
    }
    let mut sections: Vec<_> = by_section.into_values().collect();
    sections.sort_by(|a, b| {
        overview_first(&a.id)
            .cmp(&overview_first(&b.id))
            .then(a.id.cmp(&b.id))
    });
    sections
}

fn cluster_sections(files: &[String], _snapshot: &RepoSnapshot) -> Vec<PlannedSection> {
    let mut by_top: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut top_level_docs: Vec<String> = Vec::new();
    for path in files {
        match path.split_once('/') {
            Some((top, _)) if !SKIP_DIRS.contains(&top) => {
                by_top
                    .entry(top.to_string())
                    .or_default()
                    .push(path.clone());
            }
            None => top_level_docs.push(path.clone()),
            _ => {}
        }
    }

    let mut overview_files = top_level_docs.clone();
    for key in [
        "README.md",
        "ARCHITECTURE.md",
        "CONTRIBUTING.md",
        "AGENTS.md",
    ] {
        if files.iter().any(|f| f == key) && !overview_files.iter().any(|f| f == key) {
            overview_files.push(key.to_string());
        }
        if let Some(found) = files.iter().find(|f| f.ends_with(&format!("/{key}"))) {
            if !overview_files.contains(found) {
                overview_files.push(found.clone());
            }
        }
    }
    overview_files.sort();
    overview_files.dedup();
    if overview_files.is_empty() {
        overview_files.extend(files.iter().take(8).cloned());
    }

    let mut arch_files: Vec<String> = files
        .iter()
        .filter(|f| {
            let lower = f.to_ascii_lowercase();
            lower.contains("architecture")
                || lower.ends_with("lib.rs")
                || *f == "Cargo.toml"
                || f.starts_with("docs/")
        })
        .take(20)
        .cloned()
        .collect();
    if arch_files.is_empty() {
        arch_files = overview_files.clone();
    }

    let mut sections = vec![PlannedSection {
        id: "overview".into(),
        title: "Overview".into(),
        pages: vec![
            PlannedPage {
                slug: "overview".into(),
                title: "Platform Overview".into(),
                section: "overview".into(),
                source_files: overview_files,
            },
            PlannedPage {
                slug: "architecture".into(),
                title: "Architecture".into(),
                section: "overview".into(),
                source_files: arch_files,
            },
        ],
    }];

    for (top, mut paths) in by_top {
        if paths.len() < 3 {
            continue;
        }
        paths.sort();
        let mut pages: Vec<PlannedPage> = Vec::new();
        let mut by_second: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for path in &paths {
            let rest = path.split_once('/').map(|(_, r)| r).unwrap_or(path);
            let second = rest.split_once('/').map(|(s, _)| s).unwrap_or("");
            if second.is_empty() {
                by_second
                    .entry(format!("{top}-root"))
                    .or_default()
                    .push(path.clone());
            } else {
                by_second
                    .entry(second.to_string())
                    .or_default()
                    .push(path.clone());
            }
        }

        pages.push(PlannedPage {
            slug: slugify(&top),
            title: title_from_slug(&top),
            section: slugify(&top),
            source_files: paths.iter().take(16).cloned().collect(),
        });

        let mut extra: Vec<(String, Vec<String>)> = by_second
            .into_iter()
            .filter(|(k, v)| *k != format!("{top}-root") && v.len() >= 5)
            .collect();
        extra.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, files) in extra.into_iter().take(6) {
            pages.push(PlannedPage {
                slug: slugify(&format!("{top}-{name}")),
                title: title_from_slug(&name),
                section: slugify(&top),
                source_files: files.into_iter().take(16).collect(),
            });
        }

        sections.push(PlannedSection {
            id: slugify(&top),
            title: title_from_slug(&top),
            pages,
        });
    }

    sections.sort_by(|a, b| {
        overview_first(&a.id)
            .cmp(&overview_first(&b.id))
            .then(a.id.cmp(&b.id))
    });
    // Cap total pages so a default TOC stays readable (DeepWiki-like).
    let mut count = 0usize;
    for section in &mut sections {
        if count >= 20 {
            section.pages.clear();
            continue;
        }
        let remain = 20 - count;
        if section.pages.len() > remain {
            section.pages.truncate(remain);
        }
        count += section.pages.len();
    }
    sections.retain(|s| !s.pages.is_empty());
    sections
}

fn filter_source_files(files: &[String]) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for path in files {
        if path.starts_with('.') && !path.starts_with(".crew/") {
            continue;
        }
        let top = path.split('/').next().unwrap_or("");
        if SKIP_DIRS.contains(&top) {
            continue;
        }
        let ext = path.rsplit('.').next().unwrap_or("");
        if SOURCE_EXTS.contains(&ext) || path.ends_with("README") {
            out.insert(path.clone());
        }
    }
    out.into_iter().collect()
}

fn slugify(raw: &str) -> String {
    let mut s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    s.trim_matches('-').chars().take(80).collect()
}

fn title_from_slug(slug: &str) -> String {
    slug.split(['-', '_', '/'])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut chars = p.chars();
            match chars.next() {
                Some(c) => format!("{}{}", c.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn overview_first(id: &str) -> u8 {
    if id == "overview" {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_snapshot::RepoSnapshot;

    fn fixture_snapshot() -> RepoSnapshot {
        RepoSnapshot {
            commit: "aaa111".into(),
            branch: "main".into(),
            files: vec![
                "README.md".into(),
                "ARCHITECTURE.md".into(),
                "Cargo.toml".into(),
                "crates/buzz-relay/src/lib.rs".into(),
                "crates/buzz-relay/src/main.rs".into(),
                "crates/buzz-core/src/kind.rs".into(),
                "crates/buzz-core/src/lib.rs".into(),
                "crates/buzz-db/src/lib.rs".into(),
                "desktop/src/app/App.tsx".into(),
                "desktop/src/app/AppShell.tsx".into(),
                "desktop/src/features/messages/ui/MessageRow.tsx".into(),
                "desktop/src/features/sidebar/ui/AppSidebar.tsx".into(),
                "desktop/package.json".into(),
                "mobile/lib/main.dart".into(),
                "mobile/lib/shared/relay/nostr_models.dart".into(),
                "docs/crew/IDENTITY.md".into(),
                "docs/crew/DECISIONS.md".into(),
                "migrations/0001_initial_schema.sql".into(),
                "target/debug/buzz".into(),
                "node_modules/foo/index.js".into(),
            ],
            contents: BTreeMap::new(),
        }
    }

    #[test]
    fn planning_is_deterministic() {
        let snap = fixture_snapshot();
        let a = plan_pages(&snap, None).expect("a");
        let b = plan_pages(&snap, None).expect("b");
        assert_eq!(a, b);
        assert_eq!(a.language, "en");
        assert_eq!(a.sections[0].id, "overview");
        let slugs: Vec<_> = a
            .sections
            .iter()
            .flat_map(|s| s.pages.iter().map(|p| p.slug.as_str()))
            .collect();
        assert!(slugs.contains(&"overview"));
        assert!(slugs.contains(&"architecture"));
        assert!(slugs.iter().any(|s| s.contains("crates") || *s == "crates"));
        assert!(slugs.iter().any(|s| s.contains("desktop")));
        assert!(!slugs.iter().any(|s| s.contains("target")));
    }

    #[test]
    fn steering_pages_bypass_clustering() {
        let snap = fixture_snapshot();
        let steering = WikiSteering {
            language: Some("ja".into()),
            repo_notes: Some("internal".into()),
            pages: Some(vec![crate::steering::SteeringPage {
                slug: "custom".into(),
                title: Some("Custom".into()),
                section: Some("overview".into()),
                source_files: Some(vec!["README.md".into()]),
            }]),
        };
        let plan = plan_pages(&snap, Some(&steering)).expect("plan");
        assert_eq!(plan.language, "ja");
        assert_eq!(plan.sections.len(), 1);
        assert_eq!(plan.sections[0].pages[0].slug, "custom");
        assert_eq!(plan.sections[0].pages[0].source_files, vec!["README.md"]);
    }
}
