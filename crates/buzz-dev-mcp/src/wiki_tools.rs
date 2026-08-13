//! DeepWiki-compatible wiki MCP tools plus Crew `wiki_generate` / `wiki_propose`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crew_wiki::ask::{ask, AskMode, AskRequest, GroundingHit};
use crew_wiki::cadence::GenerateLock;
use crew_wiki::cluster::plan_pages;
use crew_wiki::generate::{generate_page, HeuristicGenerator};
use crew_wiki::git_snapshot::RepoSnapshot;
use crew_wiki::publish::toc_content;
use crew_wiki::steering::load_steering;
use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

fn generate_lock() -> &'static GenerateLock {
    static LOCK: OnceLock<GenerateLock> = OnceLock::new();
    LOCK.get_or_init(GenerateLock::default)
}

fn workdir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn repo_root(repo: Option<&str>) -> PathBuf {
    repo.map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(workdir)
}

fn json_error(code: &str, message: &str) -> CallToolResult {
    CallToolResult::error(vec![Content::text(
        json!({ "code": code, "message": message }).to_string(),
    )])
}

fn json_ok(value: serde_json::Value) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![Content::text(
        value.to_string(),
    )]))
}

/// Snapshot + plan for the workdir (or an explicit root).
fn plan_repo(root: &Path) -> Result<(RepoSnapshot, crew_wiki::WikiPlan), CallToolResult> {
    let snapshot =
        RepoSnapshot::from_git(root).map_err(|e| json_error(e.code(), &e.to_string()))?;
    let steering = load_steering(root);
    let plan = plan_pages(&snapshot, steering.as_ref())
        .map_err(|e| json_error(e.code(), &e.to_string()))?;
    Ok((snapshot, plan))
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct AskQuestionParams {
    pub question: String,
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct WikiRepoParams {
    #[serde(default)]
    pub repo: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct ReadWikiContentsParams {
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WikiProposeParams {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub engram_slug: Option<String>,
}

fn parse_mode(raw: Option<&str>) -> AskMode {
    match raw.unwrap_or("auto") {
        "qa" | "q&a" | "QnA" => AskMode::Qa,
        "plan" => AskMode::Plan,
        _ => AskMode::Auto,
    }
}

pub async fn ask_question(p: AskQuestionParams) -> Result<CallToolResult, ErrorData> {
    let root = workdir();
    let (snapshot, plan) = match plan_repo(&root) {
        Ok(v) => v,
        Err(err) => return Ok(err),
    };
    let mut hits = Vec::new();
    let q = p.question.to_ascii_lowercase();
    for section in &plan.sections {
        for page in &section.pages {
            let blob = page.source_files.join(" ");
            if page.title.to_ascii_lowercase().contains(&q)
                || blob.to_ascii_lowercase().contains(&q)
                || q.split_whitespace()
                    .any(|w| blob.to_ascii_lowercase().contains(w))
            {
                let excerpt = snapshot
                    .contents
                    .get(page.source_files.first().map(String::as_str).unwrap_or(""))
                    .cloned()
                    .unwrap_or_else(|| page.source_files.join(", "));
                hits.push(GroundingHit {
                    title: page.title.clone(),
                    excerpt,
                    citation: page
                        .source_files
                        .first()
                        .map(|f| format!("buzz://file?path={f}&lines=1-20")),
                });
            }
        }
    }
    if hits.is_empty() {
        if let Some(page) = plan.sections.first().and_then(|s| s.pages.first()) {
            hits.push(GroundingHit {
                title: page.title.clone(),
                excerpt: format!("Wiki page {}", page.slug),
                citation: None,
            });
        }
    }
    let response = ask(&AskRequest {
        question: p.question,
        mode: parse_mode(p.mode.as_deref()),
        hits,
        repo_d: None,
    });
    json_ok(serde_json::to_value(response).unwrap_or(json!({})))
}

pub async fn read_wiki_structure(p: WikiRepoParams) -> Result<CallToolResult, ErrorData> {
    let root = repo_root(p.repo.as_deref());
    let (snapshot, plan) = match plan_repo(&root) {
        Ok(v) => v,
        Err(err) => return Ok(err),
    };
    let manifest = crew_wiki::publish::TocManifest {
        sections: plan.sections,
        commit: snapshot.commit,
        branch: snapshot.branch,
        cadence: "manual".into(),
        generated_at: 0,
    };
    json_ok(json!({
        "kind": "toc",
        "content": toc_content(&manifest),
        "commit": manifest.commit,
    }))
}

pub async fn read_wiki_contents(p: ReadWikiContentsParams) -> Result<CallToolResult, ErrorData> {
    let root = workdir();
    let (snapshot, plan) = match plan_repo(&root) {
        Ok(v) => v,
        Err(err) => return Ok(err),
    };
    let wanted = p.slug.as_deref();
    let mut pages = Vec::new();
    let generator = HeuristicGenerator;
    for section in &plan.sections {
        for page in &section.pages {
            if wanted.is_some_and(|s| s != page.slug) {
                continue;
            }
            match generate_page(&generator, page, &snapshot, &plan.language) {
                Ok(draft) => pages.push(json!({
                    "slug": draft.slug,
                    "title": draft.title,
                    "content": draft.content,
                    "source_files": draft.source_files,
                })),
                Err(err) => return Ok(json_error(err.code(), &err.to_string())),
            }
        }
    }
    if pages.is_empty() {
        return Ok(json_error("not_found", "no matching wiki page"));
    }
    json_ok(json!({ "pages": pages }))
}

pub async fn wiki_generate(p: WikiRepoParams) -> Result<CallToolResult, ErrorData> {
    let root = repo_root(p.repo.as_deref());
    let key = root.to_string_lossy().to_string();
    let _guard = match generate_lock().acquire(&key) {
        Ok(g) => g,
        Err(err) => return Ok(json_error(err.code(), &err.to_string())),
    };
    let (snapshot, plan) = match plan_repo(&root) {
        Ok(v) => v,
        Err(err) => return Ok(err),
    };
    json_ok(json!({
        "accepted": true,
        "pages": plan.sections.iter().map(|s| s.pages.len()).sum::<usize>(),
        "commit": snapshot.commit,
        "message": "Generate accepted. Desktop worker publishes pages as events arrive."
    }))
}

pub async fn wiki_propose(p: WikiProposeParams) -> Result<CallToolResult, ErrorData> {
    let slug = p.slug.unwrap_or_else(|| slugify_title(&p.title));
    let d = format!("_proposal/{slug}");
    let mut tags = vec![
        json!(["d", d]),
        json!(["title", p.title]),
        json!(["crew-wiki-proposal", "1"]),
        json!(["crew-wiki-status", "pending"]),
    ];
    if let Some(engram) = p.engram_slug.as_ref().filter(|s| !s.is_empty()) {
        tags.push(json!(["crew-engram-slug", engram]));
    }
    json_ok(json!({
        "status": "pending",
        "kind": 30023,
        "d": d,
        "title": p.title,
        "content": p.content,
        "engram_slug": p.engram_slug,
        "tags": tags,
        "message": "Draft for owner review. Publish this kind 30023 event from the agent key; the founder republishes without the _proposal/ prefix (D-028)."
    }))
}

fn slugify_title(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    s.trim_matches('-').chars().take(80).collect()
}

#[cfg(test)]
mod tests {
    use crew_wiki::cadence::GenerateLock;

    #[test]
    fn wiki_generate_lock_rejects_parallel() {
        let lock = GenerateLock::default();
        let guard = lock.acquire("buzz").expect("first");
        let err = match lock.acquire("buzz") {
            Ok(_) => panic!("second acquire should fail"),
            Err(e) => e,
        };
        assert_eq!(err.code(), "generate_in_progress");
        drop(guard);
        lock.acquire("buzz").expect("after drop");
    }
}
