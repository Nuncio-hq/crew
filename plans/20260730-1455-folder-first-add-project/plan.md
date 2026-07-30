# Folder-first Add Project Plan

- **Status:** complete
- **Approved by:** manager request to implement the clarified Add Project flow
- **Date:** 2026-07-30

## Outcome

Make the Projects `+ → Repository` action select a local folder first, then
create the canonical NIP-34 Project and local-workspace association on Buzz's
relay. Remove the standalone Local workspace strip.

## Phases

1. [Spike and contract tests](phase-01-spike-and-contracts.md) — complete
2. [Implement and verify](phase-02-implement-and-verify.md) — complete

## Scope boundary

- TypeScript UI and existing relay/Tauri APIs only.
- Kind `30617`; local path remains location metadata.
- No Rust, clone, Git inspection, `git init`, folder mutation, session cwd,
  commit, or push.

## Key dependency

The relay and current Keychain identity must be available. The Project is not
inserted into UI state until relay acknowledgement and exact read-back pass.
