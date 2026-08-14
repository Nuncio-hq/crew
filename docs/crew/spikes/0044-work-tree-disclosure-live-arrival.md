# Spike 0044 — Disclosure / caps vs live arrival (#203)

- **Status:** PASS
- **Date:** 2026-08-14
- **Issue:** [#203](https://github.com/Nuncio-hq/crew/issues/203)

## Question

When a work-thread row arrives while a folder is collapsed, does the
badge update without auto-expanding? Do caps (`…N more`) stay stable?

## Decision affected

D-062 auto-collapse (48h quiet) and live-update face: badge only, no
auto-expand. Cap 5 + inline more.

## Hypothesis

Persisted `expanded: false` (user or auto-collapse write) wins over a
fresh last-activity timestamp. `disclosureAfterLiveArrival` keeps
collapsed. `capThreads` reports hidden count until `moreExpanded`.

## Scope

- `workTreeEligibility.ts` (`disclosureAfterLiveArrival`,
  `applyCollapsedArrival`, `capThreads`, `shouldAutoCollapse`)
- `workTreeDisclosure.ts` localStorage map
- `WorkTreeFolder` persist-on-auto-collapse effect

## Exclusions

Pin UI beyond a context-menu toggle. Mobile.

## Pass criteria

Collapsed + live arrival → `expanded: false`. Cap 8 → 5 visible + 3
hidden; `moreExpanded` shows all. Pin skips auto-collapse.

## Fail criteria

Live arrival expands a collapsed folder. Cap silently drops rows with
no `…more`. Pin ignored.

## Environment

Node unit tests. E2E mock bridge for the collapsed live-message case.

## Method

Unit: `disclosureAfterLiveArrival` then `applyCollapsedArrival` with
`lastActivityAt = now`. Cap fixtures. Auto-collapse + pin.

## Results

Collapsed disclosure stays false after a now-timestamp arrival. Cap
and more match the spec. Pin overrides 48h quiet.

## Edge cases observed

Auto-collapse without a stored disclosure would otherwise re-expand
when activity returns; the folder effect writes `expanded: false` so
the next live arrival is badge-only.

## Limitations

E2E proves the live-message path; 48h quiet is unit-only (clock).

## Verdict

PASS — collapsed stays collapsed; badge/cap are independent.

## Follow-up test contract

Unit: auto-collapse, pin, live-arrival, cap. E2E: collapse, emit
reply, assert `aria-expanded=false` and needs-you badge still visible.

## Cleanup

None.
