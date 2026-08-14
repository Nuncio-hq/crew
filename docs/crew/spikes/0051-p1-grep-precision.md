# Spike 0051 — P1 grep precision (#205)

- **Status:** PASS
- **Date:** 2026-08-14
- **Issue:** [#205](https://github.com/Nuncio-hq/crew/issues/205)

## Question

Can a static lint flag `flex-1` without `min-w-0` at a false-positive
rate low enough to wire into `just desktop-check`, without allowlisting
half the tree?

## Decision affected

D-064 heuristic lint (`check-pane-responsive.mjs`).

## Hypothesis

Most false positives are **column shells** (`flex-1 flex-col min-h-0`)
where `flex-1` grows height. Exempting `flex-col` and treating
`min-w-0` / `truncate` / `overflow-hidden` as safe drops the rate to
near zero on pane directories.

## Scope

- `scripts/check-pane-responsive-core.mjs`
- Pane roots: messages/ui, channels/ui, sidebar/ui, forum, workbench,
  work-tree, tool-pane, wiki/ui

## Exclusions

Full-screen `features/projects`, `settings`, `onboarding` (window-level
P3 allowlist). `md: any` markdown-it callbacks.

## Pass criteria

A seeded `flex-1 text-sm` fixture fails. A `flex min-h-0 flex-1 flex-col`
fixture passes. Production pane dirs are clean or explicitly
allowlisted per literal.

## Fail criteria

&gt;20 unexplained P1 hits in pane dirs, or the seed is not caught.

## Environment

Node test runner (`desktop/src/shared/layout/paneResponsiveLint.test.mjs`).

## Method

Parse `className="…"` and `cn("…")` first strings. Apply the exempt
tokens above. Seed fixtures in a temp dir (never committed to `src/`).

## Results

Column-shell exemption is the precision lever. Viewport P3 is a
separate regex (`sm:`/`md:`/`lg:`/`xl:`/`2xl:`) with a path allowlist
for true window-level files.

## Edge cases observed

`cn(variable)` and template literals are invisible to the heuristic —
same limitation as `check-px-text`. Acceptable.

## Limitations

Does not prove a flex child is text-bearing. Relies on the three-token
safe list rather than a full CSS parser.

## Verdict

PASS. Precision is good enough to gate CI on pane directories.

## Follow-up test contract

`paneResponsiveLint.test.mjs` seeded P1 + P3 + column-shell.

## Cleanup

Temp dirs removed by the test.
