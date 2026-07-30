# Phase 01 — Exact-path read-only resolver

## Overview

- **Priority:** High
- **Status:** Complete
- Derive the selected path's parent and basename in TypeScript, reuse Buzz's
  existing local snapshot command, and require exact returned-path agreement.

## Requirements

- RED tests cover renamed Project d-tag, Unicode/spaces, null/error results,
  mismatched returned path, and no remote fallback.
- Detail and overview read the same exact workspace.
- Local source becomes the default only after a valid snapshot exists.
- Linked workspace never exposes Terminal or remote Git mutation controls.

## Files

- Create focused exact-path resolver/runtime and contract tests.
- Modify `hooks.ts`, `useProjectsRepoSnapshots.ts`, and the smallest possible
  Project detail/source-control surface.
- Update Crew state, testing, architecture, changelog, and verification docs.

## Success criteria

- Exact local files, commits, contributors, README, and language data render.
- Missing, unreadable, non-Git, symlink, or mismatched paths show unavailable.
- No Rust/Cargo delta; file-size gate stays green.
- Focused/full tests, typecheck, checks, E2E build, and release build pass.
  Native exact-data smoke runs once a relay Project fixture exists.

## Completion

- [x] RED contracts demonstrated the missing exact reader and stale fallback.
- [x] Exact TypeScript resolver added with returned-path agreement.
- [x] Detail and overview use the same read-only path.
- [x] Mutation, Terminal, and configured-root commit-diff paths are disabled.
- [x] Focused contracts, full suite, typecheck, checks, and E2E build pass.
- [x] Release build passed and Computer Use opened the rebuilt app.
- [ ] Exact files/commits native smoke: current relay had no Project fixture;
  no real event was created implicitly.
