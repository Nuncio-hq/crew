# Plan — #105 Agent attention & recovery

Spec: [#105](https://github.com/Nuncio-hq/crew/issues/105) — the issue body is the
authoritative spec (Gate 0 + Slices 1–3, with per-slice definitions of done).
This file is the **execution plan only**; it deliberately does not restate the spec.

Status: **implementation in flight.**

## In-flight work

[PR #108](https://github.com/Nuncio-hq/crew/pull/108) — `feat/agent-attention-recovery`,
head `e0849ef2c`, **75 files**.

State as of 2026-08-09:

- `NuncioCrew Gate`: **SUCCESS** — all six lanes green (`CI Policy`, `Desktop Fast`,
  `Desktop Rust`, `macOS ARM Package`, `Project Relay`, `buzz-acp`).
- `mergeable: MERGEABLE`, `mergeStateStatus: UNSTABLE` (advisory e2e only).
- `Desktop Smoke E2E (1)`: FAILURE, `(4)`: CANCELLED — **both identical to `main`'s
  known failures** (#110 and #109 respectively), so neither is attributable to this PR.
- The worktree `crew/.worktrees/bring-hermes-chat-into-crew` still has uncommitted
  changes — **an agent is actively working in it.** The PR is not finished.

**Do not merge this PR from another session.** The owner is mid-work; the head will
move. Coordinate before touching it.

## What remains

Owned by the PR author, not re-planned here:

- [ ] Finish and commit the working-tree changes (`elicitation.rs`, `pool.rs`,
      `relay.rs`, `ingest.rs`, and the desktop side).
- [ ] Confirm Gate 0's spike record exists under `docs/crew/spikes/` with explicit
      PASS/FAIL criteria — the spec gates implementation on it.
- [ ] Freeze the tip long enough for one complete 4-shard e2e run before merge.

## Cross-issue interactions — read before merging

1. **Overlaps #110.** PR #108 modifies `desktop/src/app/useLiveHomeFeedActions.ts`,
   which is the exact file whose app-wide user-input subscription causes #110.
   Whichever lands second must re-verify `channels.spec.ts:500`. If #108 lands
   first, re-run the #110 bisect against the new head — the root cause may move.
2. **Blocked-by-proxy on #109.** This PR's most important claims are runtime UI
   behaviours across replay, reconnect, and restart. Unit tests and typecheck do
   not prove those. Shard 4 is dead, so the net that would catch an unwired
   projection is currently absent. That is a real, stated risk of merging now.
3. **Feeds #102.** #102 Phase 2 consumes the `needs_input` / `failed` / retry
   projections this issue defines. Landing #105 first avoids #102 re-deriving them.

## Verification bar

The spec's own definitions of done apply. Two additions that are non-negotiable
for this repo:

- **Mutation check.** For each behavioural fix, revert the fix line and the suite
  must go red. A test that exercises a new helper without proving the production
  path calls it proves nothing — this has bitten the repo before.
- **Attribute e2e by test name**, never by shard colour, and never inherit green
  from a prior head when the lane has zero completed runs at the current head.

## Open questions

- Does Gate 0's spike record exist, and did it resolve the review-action decision
  conflict the spec flags? Not verified in this session.
- 75 files is large for one PR. Worth asking the author whether the ACP/relay half
  can land separately from the desktop projection half — smaller reverts, clearer
  attribution.
