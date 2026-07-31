# Phase 04 — Local release build + computer-use verification

- **Status:** Not started
- **Priority:** high — this gates merge

## Context

Oscar chose the stable channel specifically because the build is verified by
agents driving the real app before publication, not only by CI. Per
`docs/crew/RELEASING.md`, publication is a manager action: the workflow is a
signed dry run unless the `publish` input is enabled. Oscar has authorised the
publish for `v0.0.6`.

## Requirements

1. Build and install a local NuncioCrew build from the branch.
2. Drive the real app through computer use and confirm the shipped behaviour,
   not the mock bridge.
3. Only after that: merge to `main`, then publish stable `v0.0.6`
   (immutable tag `crew-v0.0.6`).

## Checklist to exercise in the running app

1. Open a Project thread while the source checkout is behind `origin/main`;
   confirm the worktree is cut from the remote tip.
2. Confirm the strip renders as two 3-column rows and every cell opens a drawer.
3. Mention a second agent **in a reply**; confirm it joins the handoff list and
   moves through `working` → `done`.
4. With no PR on the branch, confirm row 2 collapses to `No PR yet`.
5. Open a PR from the thread, then confirm issue / PR / CI cells populate and
   their drawers show history.
6. `Remove worktree` refuses while the worktree is dirty, and succeeds once clean.
7. `Delete branch` and `Close PR` both confirm before acting.
8. Create an agent on a preset harness; confirm the vendor avatar appears.
9. Cmd +/- zoom: all strip text scales (no frozen px text).
10. Switch communities and reopen a Project thread; no PR data leaks across.

Capture screenshots for the PR — `just desktop-screenshot` for mock-bridge
views, real screen captures for the installed build. Post with
`scripts/post-screenshots.sh`; verify `shasum -a 256` is distinct per file.

## Merge and release

```bash
just ci                                   # must be green
gh pr checks <pr>                         # must be green
gh pr merge <pr> --squash                 # after Cursor's review approves
```

Then dispatch the release workflow with input `v0.0.6` and `publish` enabled,
per `docs/crew/RELEASING.md`. Do not hand-create the tag — the release helper
derives `crew-v0.0.6`.

After publication, update `docs/crew/STATE.md` with the release evidence, in
the shape the previous release used.

## Risk

A computer-use pass that only exercises the mock bridge proves nothing about
the shipped build. If the installed app cannot be driven, say so and stop
before merge rather than substituting the e2e suite for this gate.

## Rollback

If the published build is wrong, publish a corrected `v0.0.7` — the tag
`crew-v0.0.6` is immutable and must not be moved.
