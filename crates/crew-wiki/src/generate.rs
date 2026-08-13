//! Page generation — heuristic default; optional OpenAI-compatible LLM.

use crate::git_snapshot::RepoSnapshot;
use crate::publish::PageDraft;
use crate::types::PlannedPage;
use crate::WikiError;

/// Page generator. Caller-agnostic (desktop worker, CLI, future service).
pub trait Generator {
    /// Produce markdown for one planned page.
    fn generate(
        &self,
        page: &PlannedPage,
        snapshot: &RepoSnapshot,
        language: &str,
    ) -> Result<String, WikiError>;
}

/// Deterministic generator used in tests and when no API key is present.
pub struct HeuristicGenerator;

/// Select the day-one generator. OpenAI-compatible HTTP is behind the `llm`
/// feature and `CREW_WIKI_API_KEY`; without those, heuristic is canonical.
pub fn generator_from_env() -> HeuristicGenerator {
    HeuristicGenerator
}

impl Generator for HeuristicGenerator {
    fn generate(
        &self,
        page: &PlannedPage,
        snapshot: &RepoSnapshot,
        language: &str,
    ) -> Result<String, WikiError> {
        Ok(heuristic_markdown(page, snapshot, language))
    }
}

/// Generate a [`PageDraft`] with source-file metadata.
pub fn generate_page(
    generator: &dyn Generator,
    page: &PlannedPage,
    snapshot: &RepoSnapshot,
    language: &str,
) -> Result<PageDraft, WikiError> {
    let content = generator.generate(page, snapshot, language)?;
    Ok(PageDraft {
        slug: page.slug.clone(),
        title: page.title.clone(),
        section: page.section.clone(),
        source_files: page.source_files.clone(),
        commit: snapshot.commit.clone(),
        language: language.to_string(),
        content,
    })
}

fn heuristic_markdown(page: &PlannedPage, snapshot: &RepoSnapshot, language: &str) -> String {
    let lang_note = if language == "en" {
        String::new()
    } else {
        format!("<!-- language: {language} -->\n\n")
    };
    let mut mermaid_lines = String::from("flowchart TD\n");
    for (i, file) in page.source_files.iter().take(8).enumerate() {
        let id = format!("F{i}");
        mermaid_lines.push_str(&format!("  {id}[\"{}\"]\n", mermaid_escape(file)));
        if i > 0 {
            mermaid_lines.push_str(&format!("  F0 --> {id}\n"));
        }
    }
    let mut body = String::new();
    body.push_str(&lang_note);
    body.push_str(&format!("# {}\n\n", page.title));
    body.push_str(&format!(
        "This page is generated from {} source files at commit `{}`.\n\n",
        page.source_files.len(),
        snapshot.commit
    ));
    body.push_str("## System Architecture\n\n");
    body.push_str("```mermaid\n");
    body.push_str(&mermaid_lines);
    body.push_str("```\n\n");
    body.push_str("## Relevant modules\n\n");
    for file in page.source_files.iter().take(12) {
        let excerpt = snapshot
            .contents
            .get(file)
            .map(|s| s.lines().take(12).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default();
        let citation = format!("{file}#L1-12");
        body.push_str(&format!("### `{file}`\n\n"));
        body.push_str(&format!(
            "See [{citation}](buzz://file?path={file}&lines=1-12).\n\n"
        ));
        if !excerpt.is_empty() {
            body.push_str("```\n");
            body.push_str(&excerpt);
            body.push('\n');
            body.push_str("```\n\n");
        }
    }
    body
}

fn mermaid_escape(s: &str) -> String {
    s.replace('"', "'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PlannedPage;
    use std::collections::BTreeMap;

    #[test]
    fn heuristic_includes_mermaid_and_language_marker() {
        let page = PlannedPage {
            slug: "overview".into(),
            title: "Overview".into(),
            section: "overview".into(),
            source_files: vec!["README.md".into()],
        };
        let snap = RepoSnapshot {
            commit: "deadbeef".into(),
            branch: "main".into(),
            files: vec!["README.md".into()],
            contents: BTreeMap::from([("README.md".into(), "# hi\n".into())]),
        };
        let en = HeuristicGenerator.generate(&page, &snap, "en").expect("en");
        assert!(en.contains("```mermaid"));
        assert!(!en.contains("<!-- language:"));
        let ja = HeuristicGenerator.generate(&page, &snap, "ja").expect("ja");
        assert!(ja.contains("<!-- language: ja -->"));
    }
}
