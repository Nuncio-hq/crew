# Phase 3 — Restore sticky-bar e2e coverage (#32)

**Needs:** phase 1 (chips clickable) and phase 2 (drawer does not wedge).

## What was dropped and why

PR #27 (`e8aadb4bf`) narrowed `project-thread-worktree.spec.ts` while landing
thread orientation on top of the sticky bar (#29). The diff shows exactly four
losses:

| Dropped | Where it was |
|---|---|
| PR drawer open + content assertions | replaced by a `/^PR$/` chip visibility check |
| `Setup failed` assertion | workspace-error test, now only asserts the error message |
| `02-pr-history.png` artifact | after the drawer-open step |
| `Pull request` label assertions | sticky bar renames the chip to `PR` |

These were adaptations to a pre-existing `main` failure plus the sticky bar's own
UX change — not orientation-PR debt. #32 is the correct place to repay them.

## Restore list

**1. PR drawer open + contents.** Unblocked by phase 2. Open via the `PR` chip
and assert what `ProjectThreadIntegrationDrawer` renders (label map at
`ProjectThreadIntegrationDrawer.tsx:31`), fed by the spec's existing
`threadPullRequest` fixture (`project-thread-worktree.spec.ts:39-73`). No
force-clicks; if a click needs forcing, phase 1 is not done.

**2. `Setup failed` — read this before restoring it.** The string exists at
exactly one place: `ProjectThreadWorkspacePanel.tsx:362`, inside the
**focus-mode expanded grid** cell detail. The sticky bar's collapsed row and the
drawer never render it. The expanded grid only mounts when `isFocusMode` is true
(`ProjectThreadWorkspacePanel.tsx:317-335`).

So there are two honest options, and they are different products:

- **(a) Test where it lives** — assert `Setup failed` from focus mode, and keep
  the docked-drawer error assertion as the message text it actually shows.
- **(b) Treat the gap as the bug** — docked mode currently shows a failed
  workspace's *message* but never labels it failed. If a person in docked mode
  should see "Setup failed", that is a product change in the drawer or summary
  row, then the test follows.

Default to (a): it restores coverage without inventing UI. Escalate (b) to the
founder as a one-line question — it is a user-facing behaviour call, not an
implementer's call.

**3. `02-pr-history.png`.** Restore after the drawer opens, with
`waitForAnimations(page)` before the shot (mandatory per `CLAUDE.md`). Scope it
to the panel locator, not the page, and verify distinctness before posting:

```bash
shasum -a 256 test-results/thread-worktree/*.png   # every hash unique
```

`01-integration-strip.png`, `03-workspace-ready.png` and `04-full-project-thread.png`
already capture the same panel in nearby states — an unscoped shot here is the
textbook way to produce byte-identical PNGs.

**4. The CDP hang.** Phase 2 is the fix. This phase only re-checks it is gone —
if the drawer still hangs after phase 2, stop and reopen the diagnosis rather
than reaching for `force: true` or a longer timeout.

## Also worth restoring

`e8aadb4bf` reordered the second workspace assertion so the Workspace drawer is
opened *before* asserting `buzz/bbbbbbbbbbbb` and `2 behind origin/main`. Confirm
that ordering is still the honest one once the chips are fixed — those two
strings live in the collapsed summary and the drawer respectively, and the
reorder may have been another hit-target workaround.

## Validation

```bash
cd desktop && pnpm test:e2e:smoke -- tests/e2e/project-thread-worktree.spec.ts
cd desktop && pnpm test:e2e:smoke     # full smoke — scoped green hides neighbours
just ci
```

Smoke is flaky under load in this repo: attribute each failure individually
before blaming the change, and never gate the merge on a scoped run alone.

## Acceptance

- [ ] PR drawer opens by ordinary click; contents asserted
- [ ] workspace error path asserted at the surface that renders it, with option
      (b) either implemented or explicitly declined on the issue
- [ ] `02-pr-history.png` (or successor) captured, hash-distinct from siblings
- [ ] no `force: true`, no bumped timeouts, no `page.waitForTimeout` added to
      make any of this pass
