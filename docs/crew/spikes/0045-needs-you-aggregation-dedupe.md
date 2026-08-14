# Spike 0045 — Needs-you aggregation dedupe (#203)

- **Status:** PASS
- **Date:** 2026-08-14
- **Issue:** [#203](https://github.com/Nuncio-hq/crew/issues/203)

## Question

Can one pending item that matches several kinds be counted once, on
the highest-rank kind (escalation > question > approval > evidence)?

## Decision affected

D-062 Needs-you section: filter over existing stores, never counted
twice, hidden at 0.

## Hypothesis

Dedupe by id in `aggregateNeedsYou`. Same id as question + escalation
keeps the escalation hop (`Cody → Hermes`) and does not inflate
`count`.

## Scope

- `desktop/src/features/work-tree/lib/needsYouAggregation.ts`
- `needsYouAggregation.test.mjs`

## Exclusions

A new room or store. Cockpit. Daily-brief rollups.

## Pass criteria

Duplicate ids → `count === unique ids`. Escalation wins rank. Section
hidden when `count === 0` (UI).

## Fail criteria

Same request listed under two headings. Header chrome at zero.

## Environment

Node unit tests.

## Method

Two items share `id: req-1` (question + escalation); one distinct
approval. Assert count 2, escalation group 1, question group 0.

## Results

`aggregateNeedsYou` returns count 2, hop preserved, question group
empty. Distinct ids stay in their groups. `needsYouSectionLabel(2)` is
`Needs you · 2`.

## Edge cases observed

Kind rank is independent of insertion order.

## Limitations

Hop label text comes from #198 `escalationHopLabel`; this spike only
carries the string through.

## Verdict

PASS — never counted twice.

## Follow-up test contract

Unit: duplicate id across kinds. E2E: section hidden at 0; one user
input appears once and deep-links.

## Cleanup

None.
