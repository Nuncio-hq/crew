//! Ask Auto / Q&A / Plan. Grounding is supplied by the caller (NIP-50 hits).

use serde::{Deserialize, Serialize};

/// Ask mode chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AskMode {
    /// Pick Q&A vs Plan from the question shape.
    Auto,
    /// Cited answer.
    Qa,
    /// Implementation plan + start-thread payload.
    Plan,
}

/// One grounding hit (wiki page or repo file snippet).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundingHit {
    /// Display title.
    pub title: String,
    /// Body excerpt.
    pub excerpt: String,
    /// Optional `buzz://file` or page coordinate.
    pub citation: Option<String>,
}

/// Ask request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskRequest {
    /// User question.
    pub question: String,
    /// Mode.
    pub mode: AskMode,
    /// FTS / wiki hits.
    pub hits: Vec<GroundingHit>,
    /// Repo d-tag when scoped.
    pub repo_d: Option<String>,
}

/// Ask response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskResponse {
    /// Resolved mode.
    pub mode: AskMode,
    /// Markdown answer or plan.
    pub markdown: String,
    /// Citations to render as chips.
    pub citations: Vec<Citation>,
    /// Prefilled kickoff when mode is Plan.
    pub thread_draft: Option<String>,
}

/// Citation chip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    /// Label (`README.md#L9-17`).
    pub label: String,
    /// Deep link.
    pub href: String,
}

/// Synthesize an answer from grounding hits (no live LLM required).
pub fn ask(request: &AskRequest) -> AskResponse {
    let resolved = match request.mode {
        AskMode::Auto => {
            let q = request.question.to_ascii_lowercase();
            if q.contains("plan") || q.starts_with("how should we") || q.starts_with("implement") {
                AskMode::Plan
            } else {
                AskMode::Qa
            }
        }
        other => other,
    };
    let citations: Vec<Citation> = request
        .hits
        .iter()
        .filter_map(|hit| {
            hit.citation.as_ref().map(|href| Citation {
                label: hit.title.clone(),
                href: href.clone(),
            })
        })
        .collect();
    match resolved {
        AskMode::Plan => {
            let mut markdown = format!("# Plan\n\n{}\n\n", request.question);
            markdown.push_str("## Steps\n\n");
            for (i, hit) in request.hits.iter().take(6).enumerate() {
                markdown.push_str(&format!(
                    "1. Use {} — {}\n",
                    hit.title,
                    clip(&hit.excerpt, 140)
                ));
                let _ = i;
            }
            if request.hits.is_empty() {
                markdown.push_str("1. Inspect the wiki TOC and cited source files.\n");
            }
            markdown.push_str("\n## Grounding\n\n");
            for hit in &request.hits {
                markdown.push_str(&format!("- {}\n", hit.title));
            }
            let thread_draft = Some(format!(
                "{}\n\n{}",
                markdown,
                citations
                    .iter()
                    .map(|c| format!("- [{}]({})", c.label, c.href))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
            AskResponse {
                mode: AskMode::Plan,
                markdown,
                citations,
                thread_draft,
            }
        }
        _ => {
            let mut markdown = String::new();
            if request.hits.is_empty() {
                markdown.push_str("No wiki hits for that question yet. Generate the repo wiki or add a company page.\n");
            } else {
                markdown.push_str(&format!("{}\n\n", clip(&request.hits[0].excerpt, 400)));
                for hit in request.hits.iter().skip(1).take(3) {
                    markdown.push_str(&format!(
                        "- **{}:** {}\n",
                        hit.title,
                        clip(&hit.excerpt, 180)
                    ));
                }
            }
            AskResponse {
                mode: AskMode::Qa,
                markdown,
                citations,
                thread_draft: None,
            }
        }
    }
}

fn clip(s: &str, n: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= n {
        return t.to_string();
    }
    format!("{}…", t.chars().take(n).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_mode_sets_thread_draft() {
        let response = ask(&AskRequest {
            question: "How should we implement wiki citations?".into(),
            mode: AskMode::Auto,
            hits: vec![GroundingHit {
                title: "README.md#L1-8".into(),
                excerpt: "Citations use buzz://file".into(),
                citation: Some("buzz://file?path=README.md&lines=1-8".into()),
            }],
            repo_d: Some("crew".into()),
        });
        assert_eq!(response.mode, AskMode::Plan);
        assert!(response.thread_draft.is_some());
        assert_eq!(response.citations.len(), 1);
    }
}
