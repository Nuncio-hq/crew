# Spike 0039 — Page-planning quality vs a DeepWiki-style TOC (#200)

- **Status:** PASS (fixture)
- **Date:** 2026-08-13
- **Issue:** [#200](https://github.com/Nuncio-hq/crew/issues/200)

## Question

Can cluster-based planning produce a two-level TOC for this repo without
`.crew/wiki.json` that a human would accept as a starting wiki?

## Decision affected

D-061: default English cluster planning; steering is optional.

## Hypothesis

Top-level directories plus README/ARCHITECTURE form Overview, crates,
desktop, docs, and mobile sections. `target/` and `node_modules/` are
skipped. Two runs on the same snapshot are identical.

## Scope

- `crew-wiki::cluster::plan_pages`
- fixture snapshot in `cluster.rs` tests (this repo's layout, not a live
  LLM)

## Exclusions

Live Devin/DeepWiki API. This VM may not call an external planner.

## Pass criteria

Deterministic TOC. Overview first. Architecture + crates + desktop pages
present. Build artifacts absent. Steering `pages` bypasses clustering.

## Fail criteria

Random section order. Steering ignored. `target` leaked into the TOC.

## Environment

`cargo test -p crew-wiki cluster --offline`

## Method

Fixture file list mirroring NuncioCrew. `plan_pages` twice. Steering
fixture with `language: ja` and one custom page.

## Results

`planning_is_deterministic` and `steering_pages_bypass_clustering` pass.
No live Devin TOC was fetched in this environment (labeled fixture
evidence).

## Edge cases observed

Empty steering `pages` falls through to clustering. Language default is
`en`.

## Limitations

Quality vs a hosted DeepWiki TOC is a human judgment; the fixture proves
structure, not prose.

## Verdict

PASS (fixture)

## Follow-up test contract

Keep `planning_is_deterministic` and steering honor tests in `crew-wiki`.

## Cleanup

None.
