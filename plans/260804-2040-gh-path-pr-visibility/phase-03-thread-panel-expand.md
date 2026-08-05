# Phase 3 — Reach worktree detail outside focus mode

Status: **In progress** · D3 decided 2026-08-05 (Oscar): compact default, expand for detail · Depends on: —

## Problem

Worktree detail — branch name, `N behind origin/<default>`, remote-vs-local
base, "Restored from disk" — lives only in the expanded grid at
`ProjectThreadWorkspacePanel.tsx:341-383`. That grid renders under
`showExpanded = isFocusMode && expanded` (`:169`), and the chevron that toggles
`expanded` renders only when `isFocusMode` (`:318`).

In the normal right-hand thread panel `isFocusMode` is false, so there is **no
control that can set `expanded`**. The user gets the one-line summary plus the
chip row, and "Workspace" opens a drawer.

This is separate from the Phase 1 bug: it affects the worktree half, is present
whether or not `gh` works, and is a design decision rather than a defect.

## Why this is a decision, not a fix

The grid is `grid-cols-3` with three `ProjectThreadIntegrationCell`s. The side
thread panel is materially narrower than focus mode, so three columns of
label + title + detail would be cramped — plausibly why the gate exists.

Two readings, and I cannot tell which was intended from the code alone:

- **The gate is deliberate.** The drawer is the intended path to detail in the
  narrow panel, and the expanded grid is a focus-mode luxury. Then the fix is
  discoverability of the drawer, not expansion.
- **The gate is incidental.** Focus mode was built first, `isFocusMode` was the
  handy flag, and nobody revisited it. Then expansion should work everywhere
  with a responsive grid.

Oscar's report — "worktrees don't show in thread info" — is consistent with
either, which is why this ships last and separately.

## Recommendation

Allow expansion everywhere, with the grid stacking below the panel's
breakpoint. Rationale: the chips already advertise Task / Workspace / Handoff in
the narrow panel, so the information is promised there; making the promise
reachable in place is cheaper for the user than learning that a chip opens a
drawer. The drawer stays for depth.

If Oscar prefers the first reading, the alternative is smaller: make the
"Workspace" chip visibly a disclosure control rather than plain text, and stop
here.

## Files (recommendation path)

- `desktop/src/features/messages/ui/ProjectThreadWorkspacePanel.tsx`
  - `:169` — `showExpanded = expanded`.
  - `:318` — render the chevron unconditionally.
  - `:341` — grid becomes responsive: single column in the narrow panel, three
    across when there is room.
  - `:105-107` — the effect that force-collapses on leaving focus mode
    (`if (!isFocusMode) setExpanded(false)`) becomes wrong once expansion is
    allowed everywhere. Removing it means expansion persists across a focus-mode
    toggle, which is the point.
- `desktop/src/features/messages/ui/MessageThreadPanel.tsx:987` — `isFocusMode`
  stays a prop; it keeps meaning "has room for the wide layout", not "may
  expand". Do not delete it.

## A related non-issue, recorded so it is not re-litigated

The chip row is `hidden … sm:flex` (`:271`). `sm:` is a **viewport** breakpoint
(640px), not a container query, so on any normal desktop window the chips do
render. This is not part of why Oscar sees nothing, and it is not worth changing
on its own. I flagged it as a gate in my first pass on the channel; that framing
was too strong.

## Tests

- E2E in `desktop/tests/e2e/project-thread-worktree.spec.ts`: expand from the
  non-focus thread panel and assert the workspace cell shows the branch.
  `project-thread-status-expand` and `project-thread-status-expanded` test ids
  already exist (`:323`, `:339`).
- Screenshots for the PR at both widths, since this is a layout change. Use
  `just desktop-screenshot`, and check `shasum -a 256` on the output PNGs —
  identical hashes mean the two states were not actually captured differently.

## Validation

```bash
just ci
cd desktop && pnpm test:e2e:smoke
```

Plus a visual pass at the narrow panel width, which is the one the change is
for and the one automated assertions are weakest at.

## Risk and rollback

Medium — this is the only phase that changes an existing layout, and it changes
it in the surface Oscar looks at most. Contained to one component; reverting the
four line-level edits restores today's behaviour exactly.

The real risk is taste, not breakage: a three-cell grid stacked into a narrow
panel adds vertical weight above the message list, and the bar sits outside the
scroll region deliberately (`MessageThreadPanel.tsx:975-977`) so that expanding
it shrinks the visible timeline. Worth looking at before merging rather than
after.
