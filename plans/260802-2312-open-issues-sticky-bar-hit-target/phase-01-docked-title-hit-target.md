# Phase 1 — Docked panel title stops stealing sticky-bar clicks (#31)

## Context

Docked auxiliary headers overlap the content below them on purpose: the header
wrapper carries `channelChrome.negativeMargin` (`-mb-5.75rem`,
`chromeLayout.ts:59`) and the body pays it back with `contentPadding`
(`pt-5.75rem`, `chromeLayout.ts:53`). Anything inside that header row that grows
to fill the row sits on top of the overlap zone and eats clicks aimed at the
sticky project-thread bar underneath.

PR #27 (`e8aadb4bf`) fixed **one** of the two title variants — it dropped
`flex-1` from `ThreadBreadcrumb.tsx:39` and moved the sticky bar + scroll region
into a shared padded column (`MessageThreadPanel.tsx:978-993`). The other
variant is untouched: `AuxiliaryPanelHeader.tsx:381` still renders

```
<h2 className="min-w-0 flex-1 translate-y-px truncate …">
```

which is exactly the element named in the issue's Playwright failure.

### What the current title path actually renders

`ThreadPanelOrientationTitle` (`ThreadPanelOrientation.tsx:41-52`) picks the
breadcrumb when `breadcrumb && onNavigate`, else the `<h2>`. And
`buildThreadBreadcrumb` (`threadOrientation.ts:60-120`) returns a breadcrumb even
for a top-level head — a one-segment chain is still a breadcrumb — as long as the
channel name is non-empty.

So the `<h2>` fallback renders when:

- `onJumpToTimelineMessage` is not supplied (`MessageThreadPanel.tsx:951-953`),
- the channel name is empty, or
- the panel is still loading (`MessageThreadPanelSkeleton.tsx:118` always renders
  the `<h2>`).

**Consequence for the issue as written:** the e2e in `project-thread-worktree.spec.ts`
opens a thread in `#general`, so it exercises the *breadcrumb* path — the one #27
already fixed. CI green on `5c79c8cc2` therefore does **not** prove the `<h2>`
case is fixed, and it does not prove it is broken either. Repro first.

## Step 1 — repro on tip before changing anything

```bash
cd desktop && pnpm test:e2e:smoke -- tests/e2e/project-thread-worktree.spec.ts
```

Green is expected (breadcrumb path). Then force the `<h2>` path — temporarily
drop `onNavigate` in `MessageThreadPanel.tsx:951` or open a thread whose panel
has no jump handler — and confirm by hand in `just dev` whether the Workspace
chip is clickable, plus whether it is the whole chip or only part of it.

Two possible outcomes, both fine:

- **Still intercepts** → apply Step 2.
- **No longer intercepts** (the #27 padded column already cleared it) → close #31
  with the repro evidence and land only the regression test from Step 3, so the
  `<h2>` variant cannot silently regress.

Record which one happened on the issue. Do not skip this step and "fix" it
blind — an unnecessary shared-component change costs more than the check.

## Step 2 — the fix, if it reproduces

Mirror #27: drop `flex-1`, keep shrink + truncate.

`AuxiliaryPanelHeader.tsx:379-388`

```diff
-        "min-w-0 flex-1 translate-y-px truncate text-base font-semibold leading-6 tracking-tight",
+        "min-w-0 max-w-full translate-y-px truncate text-base font-semibold leading-6 tracking-tight",
```

`flex-1` is `grow-1 shrink-1 basis-0%`. Removing it leaves `shrink: 1` with
`min-w-0`, so truncation still works; only the grow-to-fill goes away.

### Blast radius — one real regression to fix in the same change

`AuxiliaryPanelTitle` is shared by five call sites. Four are safe (their trailing
controls use `AuxiliaryPanelHeaderActions`, which already carries `ml-auto` at
`AuxiliaryPanelHeader.tsx:316`):

- `ChannelManagementSheet.tsx:677`
- `MessageThreadPanelSkeleton.tsx:118`
- `ThreadPanelOrientation.tsx:51`
- `UserProfilePanelHeaderContent.tsx:42` (title block variant — its wrapper div
  at `AuxiliaryPanelHeader.tsx:355` owns `flex-1`, and the inner `<h2>` is a
  block child there, so `flex-1` on it is already inert)

**Not safe:** `ChannelWorktreesDrawerShell.tsx:62-70` puts a bare `<Button>Close</Button>`
directly after the title with no `ml-auto` and no `AuxiliaryPanelHeaderActions`
wrapper. It is pushed right *only* by the title's `flex-1`. Removing `flex-1`
slides that button next to the word "Worktrees".

Fix it properly rather than papering with `ml-auto`: wrap it in
`AuxiliaryPanelHeaderActions`, matching every other panel.

## Step 3 — regression coverage

The bug is geometric, so assert geometry, not markup. Add to the docked
project-thread e2e (or a sibling spec):

- render the docked thread panel in the `<h2>` fallback state
- assert the Workspace chip receives the click — a plain `.click()` with no
  `force`, then assert the drawer opened
- narrow panel (~380px): title truncates, Close stays reachable, chips still hit

A unit test on the class string would pass while the layout is still broken.
Do not write that one.

## Validation

```bash
cd desktop && pnpm lint && pnpm test         # unit — whole package, not scoped
cd desktop && pnpm test:e2e:smoke            # full smoke, clean build
just ci
```

`pnpm test:e2e:smoke` runs its own `pnpm build:e2e` — do not hand-build and then
run `playwright test`, and do not run it while another agent builds in this
worktree (a concurrent build swaps the served bundle mid-run).

## Risk / rollback

Shared-component class change touching five panels. Rollback is the one-line
revert plus the worktrees-drawer wrapper. Screenshot the four other panel headers
before/after if anything looks off — none of them have visual regression coverage
today, which is itself worth noting but out of scope here.
