//! CLI personality (`crew-wiki generate|status|plan`).

use crate::cluster::plan_pages;
use crate::generate::{generate_page, HeuristicGenerator};
use crate::git_snapshot::RepoSnapshot;
use crate::incremental::{material_file_set_change, regen_plan};
use crate::publish::{pages_to_publish, toc_content, PageDraft, TocManifest};
use crate::steering::load_steering;
use crate::types::WikiPlan;
use std::path::Path;

pub fn run(args: Vec<String>) -> i32 {
    let mut argv = args.iter();
    let cmd = argv.next().map(String::as_str).unwrap_or("help");
    match cmd {
        "generate" => generate_cmd(argv.cloned().collect()),
        "plan" => plan_cmd(argv.cloned().collect()),
        "status" => {
            println!("{{\"status\":\"idle\"}}");
            0
        }
        "help" | "--help" | "-h" => {
            eprintln!(
                "crew-wiki generate <repo-path> [--dry-run]\n\
                 crew-wiki plan <repo-path>\n\
                 crew-wiki status"
            );
            0
        }
        other => {
            eprintln!("unknown command: {other}");
            1
        }
    }
}

fn plan_cmd(args: Vec<String>) -> i32 {
    let Some(root) = args.first() else {
        eprintln!("usage: crew-wiki plan <repo-path>");
        return 1;
    };
    match plan_at(Path::new(root)) {
        Ok(plan) => match serde_json::to_string_pretty(&plan) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("{err}");
                4
            }
        },
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

fn generate_cmd(args: Vec<String>) -> i32 {
    let mut dry_run = false;
    let mut root: Option<String> = None;
    for arg in &args {
        if arg == "--dry-run" {
            dry_run = true;
        } else if !arg.starts_with('-') {
            root = Some(arg.clone());
        }
    }
    let Some(root) = root else {
        eprintln!("usage: crew-wiki generate <repo-path> [--dry-run]");
        return 1;
    };
    let path = Path::new(&root);
    let plan = match plan_at(path) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };
    let snapshot = match RepoSnapshot::from_git(path) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };
    let generator = HeuristicGenerator;
    let mut drafts: Vec<PageDraft> = Vec::new();
    for section in &plan.sections {
        for page in &section.pages {
            match generate_page(&generator, page, &snapshot, &plan.language) {
                Ok(draft) => drafts.push(draft),
                Err(err) => {
                    eprintln!("{err}");
                    return 4;
                }
            }
        }
    }
    let existing: Vec<PageDraft> = Vec::new();
    let to_publish = pages_to_publish(&drafts, &existing);
    let manifest = TocManifest {
        sections: plan.sections.clone(),
        commit: snapshot.commit.clone(),
        branch: snapshot.branch.clone(),
        cadence: "manual".into(),
        generated_at: 0,
    };
    if dry_run {
        let payload = serde_json::json!({
            "pages": to_publish.len(),
            "toc": toc_content(&manifest),
            "idempotent": to_publish.is_empty(),
        });
        println!("{payload}");
        return 0;
    }
    println!(
        "{{\"pages\":{},\"commit\":\"{}\"}}",
        to_publish.len(),
        snapshot.commit
    );
    let _ = material_file_set_change(&[], &snapshot.paths());
    let _ = regen_plan(&existing, &[], false);
    0
}

fn plan_at(root: &Path) -> Result<WikiPlan, crate::WikiError> {
    let snapshot = RepoSnapshot::from_git(root)?;
    let steering = load_steering(root);
    plan_pages(&snapshot, steering.as_ref())
}
