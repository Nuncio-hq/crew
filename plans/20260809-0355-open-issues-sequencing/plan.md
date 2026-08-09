# Plan index — all open issues, 2026-08-09

Six open issues. One plan file each; this file is the sequencing decision across
them. Measured against `origin/main` = `e41a1a6a4`.

| # | Title | Plan | Status |
|---|---|---|---|
| [#109](https://github.com/Nuncio-hq/crew/issues/109) | e2e shard 4 is a dead lane | [plan](../20260809-0400-e2e-shard4-revival/plan.md) | not started — **do first** |
| [#110](https://github.com/Nuncio-hq/crew/issues/110) | Agent question card never renders | [plan](../20260809-0405-channel-question-card/plan.md) | root cause proven, fix pending |
| [#111](https://github.com/Nuncio-hq/crew/issues/111) | Ratchet vs upstream-owned files | [plan](../20260809-0410-file-size-ratchet-upstream-files/plan.md) | deferred behind #109 |
| [#105](https://github.com/Nuncio-hq/crew/issues/105) | Agent attention & recovery | [plan](../20260809-0425-agent-attention-recovery/plan.md) | **in flight — PR #108** |
| [#104](https://github.com/Nuncio-hq/crew/issues/104) | Hermes first-class operations | [plan](../20260809-0420-hermes-first-class-operations/plan.md) | not started |
| [#102](https://github.com/Nuncio-hq/crew/issues/102) | Channel-first missions | [plan](../20260809-0415-channel-first-missions/plan.md) | not started |

## The one decision that orders everything

**Restore the e2e safety net before building more product on top of it.**

`Desktop Smoke E2E (4)` has been `cancelled` at the 30-minute timeout on **6 of 6
consecutive `main` runs** since `b57d26def` (#95). At `e41a1a6a4` it was killed
after **test 106 of 250** — 144 tests never ran. Shard 1 has hard-failed since
`25263120e` (#96).

The three not-started feature issues (#102, #104, #105) all ship runtime behaviour
whose acceptance criteria are explicitly about **replay, reconnect, and restart**.
None of that is provable by unit tests or typecheck. e2e is the only net that has
ever caught the *code-alive-but-unwired* class in this fork — it caught the v0.5.5
sync's only real regression when no gate job did.

Building four features while 58% of a shard reports nothing is the expensive order.

## Recommended sequence

```
1. #109  revive shard 4                    ← unblocks honest verification for everything below
2. #110  question card                     ← independent, small, user-facing bug; root cause already proven
3. #105  finish + merge PR #108            ← already gate-green; owner is mid-work
4. #111  decide the ratchet rule           ← cheap decision, prevents next-sync pain
5. #104  Hermes ops, phases 01 → 02 → 04   ← phase 03 waits for #108
6. #102  channel-first missions            ← consumes #105's projections
```

## Known couplings — the things that will bite if ignored

- **#110 ↔ #105.** PR #108 modifies `desktop/src/app/useLiveHomeFeedActions.ts`,
  the exact file whose app-wide user-input subscription causes #110. Whichever
  lands second must re-verify `channels.spec.ts:500` and re-run the bisect.
- **#104 Phase 03 ↔ #105 Slice 2.** Both define the `46040 → 46041/46042`
  clarification path. Do not build both; #108 already implements it.
- **#102 Phase 2 ↔ #105.** #102 consumes `needs_input` / `failed` / retry
  projections that #105 defines. #105 first, or they get defined twice.
- **#111 ↔ #102.** #102 attaches to `MessageThreadPanel.tsx`, which sits at
  999/1000 lines. Decide the ratchet rule before adding a large Crew delta there.

## Standing verification rules for all six

These are repo-specific and have each been learned from a real failure:

- **Mutation check.** Revert the fix line; the suite must go red. A test that
  exercises a new helper without proving the production path calls it proves
  nothing.
- **Attribute e2e by test name**, never by shard colour. "failed" = lost all 3
  attempts; "flaky" = passed on retry. Shard composition shifts whenever spec
  files are added, so shard identity is not stable.
- **Never inherit green.** A lane with zero completed runs at the current head is
  UNKNOWN, not green.
- **Scope negative claims** to where you actually looked.
- **`git merge-base --is-ancestor` lies here** — the repo squash-merges. Use
  `git diff origin/main..<branch>` instead, and `git diff $(git merge-base origin/main <tip>) <tip>`
  when the PR is BEHIND.

## Note on these plan files

#102, #104, and #105 already carry complete phase plans inside their issue bodies.
These plan files deliberately **do not copy** that content — duplicated specs drift
apart and the copy silently becomes wrong. Each file covers only what the issue
does not: current status, cross-issue coupling, Crew-specific risk, and sequencing.
The issue remains the spec.
