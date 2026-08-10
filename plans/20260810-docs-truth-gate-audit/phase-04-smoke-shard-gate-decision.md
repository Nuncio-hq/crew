---
phase: 04
title: Founder decision — smoke shards required or advisory
status: pending
priority: medium
effort: S
dependencies: ["03"]
---

# Phase 04 — Founder decision: smoke shards required or advisory

- **Issue:** #117 — problem item 3; DoD checkbox 4 (decision half)
- **Soft dependency:** the #114 owner's flake-vs-real triage (Phase 03 step 4).
  Absent it, the decision can still be made on #109/#110 evidence — say so.
- **Decision owner:** the founder. Agents present options; they do not flip the
  gate. (`review-audit-self-decision` rule: an audit is input, not an order; the
  advisory posture is an existing recorded choice — `nuncio-crew-ci.yml:248-251`.)

## Why this is a decision and not a fix

The current posture was chosen on purpose and is locked by a contract test. It is
also genuinely costly: `main` can go red in E2E and merge anyway. Both facts are
true at once, so this is a trade-off the founder owns.

## Options

| | A — keep advisory until #109/#110 close (**recommended**) | B — make shards required now | C — make required, but only shards 2 and 3 |
| - | --- | --- | --- |
| Change | none | add `desktop-smoke-e2e` to `gate.needs` and to `JOB_RELEVANCE`; drop `continue-on-error` | partial matrix in the gate |
| Effect today | `main` keeps merging with red E2E; the gap is at least documented (Phase 03) | **every desktop PR red-walls immediately** — shard 4 is cancelled at the 30-min timeout on 6/6 consecutive `main` runs (#109) and shard 1 has hard-failed since `25263120e` (#96, tracked as #110) | avoids the two known-broken lanes while restoring some blocking signal |
| Cost | the safety net stays off; the regression #109 describes (composer-clear atomicity during the v0.5.5 sync) could recur unseen | development stops until #109 and #110 are fixed | encodes "which shards are trustworthy" into CI — brittle, since sharding is by count and a spec's shard assignment moves whenever the suite changes |
| Reversibility | high | high but disruptive while red | medium — needs re-tuning on every suite change |

**Recommendation: A.** Making a lane required while it is known-broken converts a
documented gap into a hard block on all desktop work, and it does not fix a single
test. The ordering #112 already argued for still holds: restore the lane
(#109 → #110), *then* make it required. The value of this issue's audit is that
the gap is now written down instead of implicit.

Treat B as the right end state, gated on #109 and #110 closing.

## Steps (only if the founder chooses B or C)

**RED first — D-008 (`Spike → RED tests → implementation`).** The advisory posture
is asserted by an existing test, so the test changes before the workflow does:

1. Rewrite `desktop/src/testing/nuncio-crew-ci-contract.test.mjs:130-156` to assert
   the new contract — replace the two negative assertions
   (`:150` `assert.doesNotMatch(ci, /needs\.desktop-smoke-e2e\.result/)` and `:155`
   the same for the gate helper) with positive ones, and drop the
   `continue-on-error: true` assertion at `:141`.
2. Run `pnpm -C desktop test` and **observe it fail**. Record the failure output —
   that is the RED evidence. A test that passes before the change proves nothing.
3. Only then edit `.github/workflows/nuncio-crew-ci.yml`: remove
   `continue-on-error: true` (`:258`), add `desktop-smoke-e2e` to `gate.needs`
   (`:320`) and to the gate payload (`:329`), and replace the advisory comment at
   `:248-251` with the new rationale.
4. Add `"desktop-smoke-e2e": "desktop"` to `JOB_RELEVANCE` in
   `desktop/scripts/check-nuncio-crew-ci-results.mjs:6-12`.
5. Re-run `pnpm -C desktop test` — green.
6. Append the decision to `docs/crew/DECISIONS.md` at the next free ID (**D-029**
   if Phase 01 took D-028 — re-check the tail). Record the trade-off and the
   evidence, not just the outcome.
7. Update `docs/crew/CI.md` (the table Phase 03 corrected) and — because this
   changes the merge gate — **update `docs/crew/STATE.md` in the same PR** per the
   Phase 01 anti-drift rule. This is the rule's first real exercise.

**Expected diff size:** ~6 lines in `nuncio-crew-ci.yml`, 1 line in
`check-nuncio-crew-ci-results.mjs`, ~10 lines in the contract test, plus docs. All
four files are Crew-only — **0 upstream lines**. Do not touch
`.github/workflows/ci.yml`; upstream's gate already aggregates smoke and its
behavior is not Crew's to change (D-020: nothing goes to `block/buzz`).

## Steps (if the founder chooses A)

1. Record the decision in `docs/crew/DECISIONS.md` **only if** the founder wants it
   sticky. "Keep the status quo for now" may not warrant a decision entry; ask.
2. Otherwise, note the outcome in the Phase 03 verification record and close the
   DoD item — the issue's wording is "founder decision recorded **if a change is
   made**".
3. Consider filing the "make required" work as a follow-up issue blocked on #109
   and #110, so the end state is not lost. Ask before filing.

## Validation

- If B/C: contract test observed RED before the workflow edit, green after; a full
  `NuncioCrew CI` run on the branch shows the gate now consuming the shard results.
- If A: the decision and its rationale are readable by the next agent without
  re-deriving the evidence.
- Either way: no `block/buzz` PR, no upstream file edited.

## Risk and rollback

- **Risk (B/C):** immediate `main` red-wall. Mitigation: the recommendation is A;
  if B is chosen anyway, sequence it after #109/#110 land.
- **Risk:** an agent applies B because "audits recommend hardening". Mitigation:
  this phase requires an explicit founder choice; the plan's default is A.
- **Rollback:** revert the workflow/script/test commit — the gate returns to
  advisory within one PR, and the contract test pins whichever posture is current.
