//! One-shot child used by cross-process lease tests.
//!
//! Exit codes: 0 = acquired, 2 = Busy, 3 = other error.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use buzz_worktree::{try_acquire_exclusive, try_acquire_shared, LeaseError};

fn main() -> ExitCode {
    let mode = env::var("BUZZ_WORKTREE_LEASE_CHILD").unwrap_or_default();
    let common = PathBuf::from(env::var("BUZZ_WORKTREE_LEASE_COMMON").unwrap_or_default());
    let root = env::var("BUZZ_WORKTREE_LEASE_ROOT").unwrap_or_default();
    let hold_ms: u64 = env::var("BUZZ_WORKTREE_LEASE_HOLD_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let result = match mode.as_str() {
        "shared" => try_acquire_shared(&common, &root).map(|lease| {
            if hold_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(hold_ms));
            }
            drop(lease);
        }),
        "exclusive" => try_acquire_exclusive(&common, &root).map(|lease| {
            if hold_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(hold_ms));
            }
            drop(lease);
        }),
        _ => {
            eprintln!("unknown lease child mode");
            return ExitCode::from(3);
        }
    };

    match result {
        Ok(()) => ExitCode::from(0),
        Err(LeaseError::Busy) => ExitCode::from(2),
        Err(error) => {
            eprintln!("lease child error: {error}");
            ExitCode::from(3)
        }
    }
}
