# Phase 01 — Spike and contract tests

## Status

Complete

## Requirements

- Prove folder picking and relay creation are possible without Rust.
- Record the arbitrary-folder Git inspection limitation.
- Write failing contracts before production code.

## Related files

- `docs/crew/spikes/0005-folder-first-project-create.md`
- `desktop/src/features/projects/project-add-local-workspace-contract.test.mjs`
- `desktop/src/features/projects/project-add-local-workspace-relay-contract.test.mjs`
- `desktop/src/features/projects/project-add-local-workspace-ui-contract.test.mjs`

## Success criteria

- The spike has an explicit verdict.
- Contracts cover path/name policy, relay fail-closed behavior, retries,
  duplicates, empty state, and removal of the standalone strip.
