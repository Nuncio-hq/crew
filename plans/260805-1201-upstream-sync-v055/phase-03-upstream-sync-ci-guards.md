# Phase 3 — Add three upstream guards to the Upstream Sync lane

Status: **Ready** (approved) · Depends on: Phase 1

## Problem

Crew inherits relay and DB code from #4671 (NIP-MP) whose only automated guard
lives in upstream's `ci.yml` — a workflow Crew has `disabled_manually` by
deliberate decision (D-017, `docs/crew/CI.md`), verified with
`gh workflow list --all`.

Upstream added three guards between `desktop-v0.5.3` and `desktop-v0.5.5` that
protect exactly the model Crew is adopting.

## Approach

Copy the three proven blocks from upstream `ci.yml` into the **manual**
`NuncioCrew Upstream Sync` workflow. Do not enable upstream's `CI` — that would
reverse D-017 and pay for Windows, Linux, Intel, sprig, helm, and k8s targets
Crew does not ship.

These are additive to the approved path-gated `Desktop Rust` job (issue #41) and
do not touch its design. Different lane, different trigger.

## Files

Modify:

- `.github/workflows/nuncio-crew-upstream-sync.yml`

## Steps

1. Copy from upstream `.github/workflows/ci.yml`, preserving the pinned action
   SHAs and the `--archive-file` nextest invocation shape:
   - **Workspace profile kind:9033 gate tests** —
     `-E 'package(buzz-relay) and test(/handlers::relay_admin::tests/)'
     --run-ignored ignored-only`
   - **NIP-MP coordinate-deletion guard** —
     `-E 'package(buzz-db) and test(coordinate_delete_spares_head_newer_than_the_deletion)'
     --run-ignored ignored-only`
   - **`e2e_project`** — added to the existing
     `cargo test -p buzz-test-client --test …` line.
2. Both nextest steps need `DATABASE_URL` and a real Postgres service, matching
   upstream's job. Confirm the Upstream Sync lane already provisions one; if it
   does not, that is a real scope addition — surface it rather than stubbing the
   tests out.

## Validation

- Run `NuncioCrew Upstream Sync` manually on `sync/upstream-2026-08-05` and
  confirm all three steps execute (not skipped) and pass.
- Confirm `NuncioCrew Gate` on ordinary feature PRs is unchanged — this workflow
  is manual-only and must not appear as a required check.

## Risk

If the Upstream Sync lane has no Postgres service, these steps fail to run
rather than fail loudly, producing a false sense of coverage. Step 2 checks that
before the guards are trusted.

## Rollback

Revert the workflow file. The lane is manual, so nothing on `main` depends on it.
