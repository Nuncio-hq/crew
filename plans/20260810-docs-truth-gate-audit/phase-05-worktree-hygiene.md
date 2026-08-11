---
phase: 05
title: Worktree hygiene with an unmerged-work safety gate
status: pending
priority: low
effort: S
dependencies: []
---

# Phase 05 — Worktree hygiene with an unmerged-work safety gate

- **Issue:** #117 — problem item 4 ("Minor"); DoD checkbox 5
- **PR scope:** none. This phase changes **local git state only** — no repo files,
  no commit, nothing to merge.
- **Split:** 05a is safe and unblocked. 05b is gated and may end in "do nothing".

## Correction to the issue's premise (read before acting)

The issue describes `.worktrees/bring-hermes-chat-into-crew` as "still checked out
on **merged** `fix/agent-attention-recovery-hardening`". Verified 2026-08-10:
**that branch is not fully merged.**

```
$ git rev-list --count origin/main..59ab743ef
6
$ git merge-base --is-ancestor 59ab743ef origin/main   → NOT an ancestor
$ git merge-base --is-ancestor 59ab743ef origin/fix/agent-attention-postmerge-audit → NO
```

The six commits ahead of `origin/main`:

```
59ab743ef fix(agent): preserve concurrent session authority
08beb03a6 fix(agent): fail closed on untrusted attention input
142e3463a fix(desktop): satisfy session size ratchet
47fc7b5d1 fix(agent): project reviewed receipt after publish
1d9a37221 fix(agent): hide completed receipt without active turn
17b4353bc fix(agent): close attention recovery gaps      ← merged as #113 (304173e42, squashed)
```

`git cherry` marks **all six** as having no upstream equivalent, including
`17b4353bc`, whose content demonstrably merged as #113. That is the known
squash-merge blind spot: squashing rewrites the patch-id, so `git cherry` cannot
distinguish "merged via squash" from "never merged". **Do not use `git cherry` as
the safety check here.** The five commits above `17b4353bc` are post-#113 work, and
PR #114 (`fix/agent-attention-postmerge-audit`) is a *different* branch whose
commits carry different subjects — overlap is plausible but unproven.

Removing this worktree and branch without reconciliation risks destroying work.
`UPSTREAM-SYNC.md:138-141` is explicit: destructive git recovery requires approval.

## Current state (2026-08-10)

- 38 worktrees registered on the main checkout.
- 26 under `$TMPDIR/buzz-ae-e2e-*/mission-worktree`, created by
  `desktop/tests/e2e/helpers/twoRelayHarness.ts:36` (`mkdtemp(join(tmpdir(),
  "buzz-ae-e2e-"))`) and leaked when a run aborts. **24 are marked `prunable`**
  (directory already gone). Two — `…-On5C8H`, `…-VlMJct` — are **not** prunable,
  i.e. their directories still exist and may belong to a live run.
- Several `/private/tmp/crew-*` review checkouts, including
  `/private/tmp/crew-postmerge-audit` on PR #114's branch — **in use, leave alone**.
- `.worktrees/` holds this planning worktree plus `issue-116-agent-roles` (PR #120,
  active), `brainstorm-coding-app`, `integrate-browser-and-mobile-emulator-into-crew`,
  and the `bring-hermes-chat-into-crew` case above.

Note the main checkout is currently on `docs/founder-product-north-star`, not
`main` — do not "fix" that as part of hygiene.

## 05a — Safe prune (unblocked)

`git worktree prune` only removes registrations whose directory is already gone.
It cannot delete a live checkout or any branch.

```bash
cd /Users/a1241968/Desktop/Oscar/LilGroup/Nuncio/crew
git worktree list | grep prunable | wc -l    # expect ~24 before
git worktree prune -v
git worktree list | wc -l                    # expect ~14 after
```

Leave the two non-prunable `buzz-ae-e2e-*` entries alone — a live e2e run may own
them. Re-check after any in-flight run finishes; they become prunable on their own.

## 05b — Gated: `.worktrees/bring-hermes-chat-into-crew`

**Default action: none.** Proceed only when all three hold:

1. The #114 line has closed (merged or abandoned), so its branch is final.
2. A content reconciliation shows the five post-#113 commits are represented on
   `origin/main` — compare trees over the touched paths rather than patch-ids:
   ```bash
   git diff --stat origin/main 59ab743ef -- desktop/src crates docs/crew
   git log --oneline --name-only origin/main..59ab743ef
   ```
   Anything left that is not on `main` is unmerged work.
3. The owner of that session confirms the branch is disposable.

If any check fails, **stop and report** — do not remove. Preserving the branch
costs one stale directory; losing hardening work costs a re-audit. If all three
pass:

```bash
git worktree remove /Users/a1241968/Desktop/Oscar/LilGroup/Nuncio/crew/.worktrees/bring-hermes-chat-into-crew
# branch deletion is a separate, later decision — the worktree can go while the branch stays
```

Do not pass `--force`. If `git worktree remove` refuses because the checkout is
dirty, that refusal *is* the safety gate working: report the dirty files.

Also note `/private/tmp/crew-hardening-verify` sits on the same commit
(`59ab743ef`, detached). Same reasoning applies; it is a temp path and lower risk.

## Validation

- `git worktree list` shows no `prunable` entries afterwards.
- No branch was deleted in this phase.
- Every remaining worktree is either active work or explicitly justified.
- `git worktree list` on the main checkout still shows the PR #114 and PR #120
  worktrees intact.

## Risk and rollback

- **Risk:** data loss from removing unmerged work — the whole point of 05b's gate.
  Mitigation: default is no action; three independent checks; no `--force`.
- **Risk:** pruning a live e2e run's worktree mid-test. Mitigation: `prune` cannot
  touch existing directories; the two non-prunable entries are left alone.
- **Rollback:** a pruned registration is recreatable with `git worktree add`; the
  underlying commits are unaffected. A removed *checkout* with uncommitted changes
  is **not** recoverable — hence the gate.

## Out of scope (flagged, not done)

The temp-worktree leak recurs because the e2e harness does not clean up on abort
(`desktop/tests/e2e/helpers/twoRelayHarness.ts:36`). Fixing that would stop
recurrence, but the file is upstream-shared and the issue asks only for a prune.
Raise as its own issue if the founder wants the recurrence fixed.
