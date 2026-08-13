# Spike 0040 — Incremental regen set across rename/move (#200)

- **Status:** PASS (fixture)
- **Date:** 2026-08-13
- **Issue:** [#200](https://github.com/Nuncio-hq/crew/issues/200)

## Question

Given old and new commits, which wiki pages must regenerate, including
when a source file is renamed or moved?

## Decision affected

D-061: page source-file manifest + commit is the regen unit.

## Hypothesis

`git diff --name-status` rename (`R`) matches both old and new paths.
Pages whose `source` tags intersect the touched set regenerate. A >20%
path change or new top-level directory re-plans the TOC. Unchanged pages
are not republished (idempotent).

## Scope

- `crew-wiki::git_snapshot::parse_name_status`
- `crew-wiki::incremental::{regen_plan, material_file_set_change}`
- `crew-wiki::publish::pages_to_publish`

## Exclusions

Live git push against a remote. Fixture name-status lines are enough.

## Pass criteria

Rename of `src/a.rs` → `src/b.rs` marks the page that listed `src/a.rs`.
Re-run without a diff publishes nothing.

## Fail criteria

Rename drops the page from the regen set. No-op republishes identical
content.

## Environment

`cargo test -p crew-wiki incremental publish --offline`

## Method

Fixture `PageDraft`s + `FileChange { kind: Rename, old_path, path }`.
`pages_to_publish` with identical drafts.

## Results

Rename hits the regen set. Idempotent publish is empty. Material file-set
change fires on a new top-level directory.

## Edge cases observed

Copies (`C`) count as the new path only. Deletes still invalidate pages
that listed the old path.

## Limitations

This VM did not replay a real 30618 push; debounce is unit-tested in
`cadence.rs`.

## Verdict

PASS (fixture)

## Follow-up test contract

Keep rename + idempotency tests in `crew-wiki`.

## Cleanup

None.
