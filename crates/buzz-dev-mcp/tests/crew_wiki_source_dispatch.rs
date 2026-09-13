#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "crew-buzz-dev-mcp-source-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).expect("create unique Git fixture");
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove only owned Git fixture");
    }
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .expect("run fixture Git");
    assert!(output.status.success(), "fixture Git failed");
}

#[test]
fn crew_wiki_argv0_dispatch_reads_from_retained_directory_fd() {
    use std::os::unix::process::CommandExt;

    let fixture = Fixture::new();
    let selected = fixture.0.join("selected");
    std::fs::create_dir(&selected).expect("create selected fixture");
    git(&selected, &["init", "--quiet"]);

    let directory = std::fs::File::open(&selected).expect("open selected directory");
    std::fs::rename(&selected, fixture.0.join("original")).expect("move selected fixture");
    std::fs::create_dir(&selected).expect("create replacement directory");

    let path = std::env::var_os("PATH").unwrap_or_default();
    let output = Command::new(env!("CARGO_BIN_EXE_buzz-dev-mcp"))
        .arg0("crew-wiki")
        .args(["__source-git", "rev-parse", "--is-inside-work-tree"])
        .env_clear()
        .env("PATH", path)
        .stdin(Stdio::from(directory))
        .output()
        .expect("run bundled multicall helper");

    assert!(output.status.success(), "multicall source read failed");
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "true");
}

#[test]
fn crew_wiki_argv0_dispatch_rejects_unallowlisted_source_read() {
    use std::os::unix::process::CommandExt;

    let fixture = Fixture::new();
    git(&fixture.0, &["init", "--quiet"]);
    let directory = std::fs::File::open(&fixture.0).expect("open owned directory");
    let path = std::env::var_os("PATH").unwrap_or_default();
    let output = Command::new(env!("CARGO_BIN_EXE_buzz-dev-mcp"))
        .arg0("crew-wiki")
        .args(["__source-git", "rev-parse", "--git-dir"])
        .env_clear()
        .env("PATH", path)
        .stdin(Stdio::from(directory))
        .output()
        .expect("run bundled multicall helper");

    assert!(
        !output.status.success(),
        "unallowlisted source read must fail closed"
    );
}
