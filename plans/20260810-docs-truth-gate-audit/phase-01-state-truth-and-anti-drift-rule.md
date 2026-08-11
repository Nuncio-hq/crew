---
phase: 01
title: STATE.md truth refresh + anti-drift rule
status: pending
priority: high
effort: M
dependencies: []
---

# Phase 01 — `STATE.md` truth refresh + anti-drift rule

- **Issue:** #117 — problem item 1; DoD checkboxes 1 and 2
- **PR scope:** docs only. No code, no CI config, no runtime behavior.
- **Files:** `docs/crew/STATE.md`, `docs/crew/AGENT-WORKING-AGREEMENT.md`,
  `docs/crew/DECISIONS.md`
- **Upstream files touched:** none (all three verified absent from `upstream/main`)

## Context

`docs/crew/STATE.md:16` claims the implementation slices below it "remain the
code truth for what is built today", and `IDENTITY.md:39` sends every agent there
for current fork state. The file is wrong in at least six places, so agents
sequence work off false state. This has recurred with no rule preventing it.

## Stale-claim inventory (verified 2026-08-10)

| `STATE.md` | Claims | Observable reality | Source |
| ---------- | ------ | ------------------ | ------ |
| `:180-182` ("Current gate") | the `0.0.6` branch "is not merged", `crew-v0.0.6` "is not published", `0.0.5 → 0.0.6` updater relaunch pending | `crew-v0.0.6` published 2026-08-01; `crew-v0.0.9` is **Latest** since 2026-08-07 | `gh release list --repo Nuncio-hq/crew` |
| `:222` | "No `crew-v0.0.6` tag or public `0.0.6` artifact has been created" | same as above — four releases past it | `gh release list` |
| `:94-95` | Buzz source pin `0.5.3` at `3a96acea09b4…` | `0.5.7` at `f167818d25dd…` | `docs/crew/upstream-buzz.json` |
| `:72` | Settings displays `v0.5.3 · Local` | version is `0.5.7` | `desktop/package.json:4`, `desktop/src-tauri/tauri.conf.json:4` |
| attention/recovery line | absent | shipped through #108 (`6793c86da`) and #113 (`304173e42`); #114 open | `git log --oneline origin/main` |
| roles track | absent | issue #116 is the head; PR #120 open | `gh issue list`, `gh pr list` |
| Hermes track `:255-257` | "Next gates: Slice 2 …" | still accurate — Slice 2 not merged | verified, leave as-is |

## Steps

1. Rewrite `## Current gate` (`STATE.md:177-185`) to state the real release
   position: releases published through `crew-v0.0.9` (2026-08-07), the
   thread-worktree `0.0.6` line merged and released, and whatever updater
   verification genuinely remains — do **not** carry the `0.0.5 → 0.0.6` phrasing
   forward if the newer releases superseded it. If the updater relaunch was never
   verified on any pair, say that plainly instead of dropping the obligation.
2. Fix `:222` in `## Current test gate` the same way.
3. Correct the Buzz source pin (`:94-95`) and the Settings version string (`:72`)
   to match `upstream-buzz.json` and `desktop/package.json`. Prefer pointing at
   `upstream-buzz.json` as the machine-readable source over restating the numbers,
   so this line cannot drift again.
4. Add an attention/recovery line to the implementation record: merged through
   #113; **PR #114 open as a follow-up at time of writing** (name it as in-flight,
   not as shipped — see `AGENT-WORKING-AGREEMENT.md:40` on not hiding open work).
5. Add the Hermes track's current position (Slice 0–1 complete, Slice 2 next —
   already at `:255-257`, verify rather than duplicate) and reference issue #116
   as the roles track head with PR #120 in flight.
6. Stamp `Last updated:` with the real merge-day date.
7. Add the anti-drift rule to the **implementation checklist** at
   `AGENT-WORKING-AGREEMENT.md:81-87`, as a new checkbox in the existing list
   style:
   > - [ ] Shipped state changed (release published, slice merged, gate changed)
   >   → update [`STATE.md`](STATE.md) in the **same** PR
8. Append the rule to `docs/crew/DECISIONS.md` as the next free ID (**D-028** as
   of 2026-08-10 — re-check the tail before writing, PR #120 may land one first).
   Per `AGENT-WORKING-AGREEMENT.md:87`, a new sticky choice gets a decision entry.
   Status Accepted, dated, linking the working agreement.

## Contracts

| Scenario | Expected result | Forbidden |
| -------- | --------------- | --------- |
| Agent reads `STATE.md` to sequence work | every release/version/merge claim matches live repo state on the merge date | inventing a release, slice, or verification that did not happen |
| Agent opens a PR that publishes a release or changes the gate | checklist tells them to update `STATE.md` in that PR | rule living only in a plan file or PR description |
| In-flight work (#114, #120) | named as open, with its state | described as shipped |

## Validation

Spot-check every assertion in the refreshed file against live state on the PR's
head — this is the issue's own verification bar ("no claim in the refreshed file
contradicts observable repo state"):

```bash
gh release list --repo Nuncio-hq/crew --limit 10
git log --oneline origin/main -10
gh pr list --repo Nuncio-hq/crew --state open
cat docs/crew/upstream-buzz.json
grep -n '"version"' desktop/package.json
```

Then `just ci` on the branch. Docs-only paths mean the desktop jobs skip and
`NuncioCrew Gate` accepts the deliberate skips (`docs/crew/CI.md:11-13`).

**This PR must itself satisfy DoD checkbox 1** — it changes `STATE.md`, so it is
trivially compliant with the rule it introduces; state that in the PR body.

## Risk and rollback

- **Risk:** the file is re-stale by merge time (PRs #114/#120 in flight).
  Mitigation: write state as-of a named date with open PRs listed as open; re-run
  the spot-check on the exact merge head.
- **Risk:** over-editing turns a state record into a changelog. Mitigation: keep
  the existing section structure; change claims, not organization.
- **Rollback:** docs-only single PR — `git revert` restores prior text with no
  runtime effect.
