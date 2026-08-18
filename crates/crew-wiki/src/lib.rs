//! Crew Wiki engine — planning, generation, incremental regen, publish.
//!
//! Separable by architecture: this crate is the library. Faces are
//! `buzz-dev-mcp` MCP tools, argv0 `crew-wiki`, and the desktop governed
//! worker. Storage is relay events ([`buzz_core::kind::KIND_REPO_WIKI_PAGE`]).

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod ask;
pub mod cadence;
pub mod cluster;
pub mod error;
pub mod generate;
pub mod generate_root;
pub mod git_snapshot;
pub mod incremental;
pub mod publish;
pub mod steering;
pub mod types;

pub use ask::{ask, AskMode, AskRequest, AskResponse, Citation};
pub use cadence::{debounce_due, next_cadence_due, CadenceClock, ON_PUSH_DEBOUNCE};
pub use cluster::plan_pages;
pub use error::WikiError;
pub use generate::{generate_page, generator_from_env, Generator, HeuristicGenerator};
pub use generate_root::{
    classify_from_git_failure, resolve_wiki_generate_root, WikiGenerateRoot, WikiLocalSnapshotError,
};
pub use git_snapshot::{list_source_files, FileChange, FileChangeKind, RepoSnapshot};
pub use incremental::{material_file_set_change, regen_plan, RegenPlan};
pub use publish::{
    page_event_tags, pages_to_publish, toc_content, toc_event_tags, PageDraft, PublishBatch,
    TocManifest,
};
pub use steering::{load_steering, WikiSteering};
pub use types::{PlannedPage, PlannedSection, WikiPlan};

/// CLI entry used by argv0 multicall and the `crew-wiki` binary.
pub fn run_cli(args: Vec<String>) -> i32 {
    cli::run(args)
}

mod cli;
