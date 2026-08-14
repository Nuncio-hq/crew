# Spike 0050 — Letter-soup detection heuristic (#205)

- **Status:** PASS
- **Date:** 2026-08-14
- **Issue:** [#205](https://github.com/Nuncio-hq/crew/issues/205)

## Question

Can an E2E helper reliably flag vertical letter-soup (one character per
line in a ~1ch column) without failing on badges, timestamps, or
ellipsis truncation?

## Decision affected

D-064 `assertPaneResponsive` letter-soup clause.

## Hypothesis

A text node whose box is narrower than 6ch **and** taller than ~2.2
line-heights is soup. Truncated (`text-overflow: ellipsis`), `sr-only`,
and `overflow-x-auto` nodes are exempt. Count badges (≤3 glyphs) are
exempt.

## Scope

- `desktop/tests/helpers/assertPaneResponsive.ts`
- Declared-plans / empty-state reproduction at 300–340px aux width

## Exclusions

OCR. Pixel-diff of glyphs. Windows below 800×500.

## Pass criteria

The helper fails on a squeezed untruncated paragraph and passes on a
truncated title plus a `99+` unread badge.

## Fail criteria

Helper flags every timestamp, or misses a 1ch-wide untruncated word.

## Environment

Playwright mock-bridge, viewport 1280×720, aux pane 300–340px.

## Method

Walk text nodes inside the pane. Measure `getBoundingClientRect` vs
`fontSize * 0.5` as a `ch` approximation. Skip ellipsis / hidden.

## Results

6ch floor matches the issue. Skipping ellipsis avoids P4 titles that
**correctly** truncate. Skipping length &lt; 4 keeps count badges.

## Edge cases observed

Empty-state container queries must use the empty-state box width
(`w-full`), not shrink-wrapped title width, or the narrow variant never
trips.

## Limitations

`ch` is approximated from `fontSize`, not the glyph “0”. Code blocks
with `overflow-x-auto` are exempt by design.

## Verdict

PASS. Heuristic is good enough for the shared helper.

## Follow-up test contract

`responsive-matrix.spec.ts` empty-state + plans-rail cases.

## Cleanup

None.
