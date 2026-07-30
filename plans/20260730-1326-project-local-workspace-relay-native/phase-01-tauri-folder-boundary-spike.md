# Phase 1 — Tauri folder-boundary spike

## Overview

- Priority: blocking
- Status: complete
- Purpose: prove the smallest supported folder picker before production code.

## Context

- `desktop/src-tauri/src/lib.rs` already initializes `tauri-plugin-dialog`.
- `desktop/src-tauri/capabilities/default.json` already grants
  `dialog:default`.
- The JavaScript dialog package is not installed.
- Tauri documents that a selected path is added to runtime scope, but that
  scope is cleared on app restart.
- The filesystem plugin and arbitrary-path capabilities are not installed.

## Spike question

Can the supported `@tauri-apps/plugin-dialog` API select exactly one existing
directory and return its raw absolute path without any Rust edit?

## Steps

1. Run the package and UI probe in a disposable worktree.
2. Select folders with spaces and non-ASCII characters.
3. Verify cancel returns no path and creates no relay event.
4. Verify the returned value is an absolute raw path, not `file://`.
5. Restart the probe and confirm the runtime scope is not persisted.
6. Delete the disposable changes and record `PASS`, `FAIL`, or `INCONCLUSIVE`
   in `docs/crew/spikes/0003-tauri-project-folder-picker.md`.

## Pass

- Folder selection works on the real Tauri desktop shell.
- No Rust or capability edit is required.
- Cancellation and path encoding match the RED contracts.

## Fail

- Selection needs an undocumented raw invoke.
- Selection needs Rust changes.
- The returned value cannot preserve the chosen absolute path.

## Decision on failure

Stop. Do not substitute a text field or browser-only picker. Present a new
manager decision between a narrow Rust adapter and deferring folder selection.

## Security

Do not broaden filesystem scope. The picker only chooses a directory; this
slice does not read source files.

## Result

PASS. The manager approved the dependency-file exception and the real Tauri
picker passed cancel, Unicode, spaces, and relink checks without a Rust edit.
