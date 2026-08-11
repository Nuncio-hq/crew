# Docs truth + gate audit (issue #117)

- **Status:** Planned — not started
- **Date:** 2026-08-10
- **Issue:** [#117](https://github.com/Nuncio-hq/crew/issues/117)
- **Repo:** `Nuncio-hq/crew` only — no PR ever targets `block/buzz` (D-020)
- **Type:** docs truth + CI posture audit + repo hygiene. **No product feature.**
- **Branch (proposed):** `docs/state-truth-and-gate-audit`
  (area-prefixed per `UPSTREAM-SYNC.md` § Feature branches; not a phase number)

## Goal

Make the docs agents plan from tell the truth, make "shipped state changed
without `STATE.md` updated" a review-visible violation, resolve the pre-north-star
plan PR #112 instead of letting it drift, write down what the merge gate does and
does not prove about E2E, and clean git worktree residue **without destroying
unmerged work**.

## Outcome in founder language

Today an agent that reads `docs/crew/STATE.md` is told `crew-v0.0.6` is
unpublished and the thread-worktree branch is unmerged. Both are false —
`crew-v0.0.9` has been the Latest release since 2026-08-07. Agents sequence work
off that file, so a stale file produces wrong plans. After this work: the file
matches what `gh release list` and `git log origin/main` show, a written rule
makes the next drift a review finding, and nobody has to guess whether a green
`NuncioCrew Gate` means E2E passed (it does not, on purpose).

## Scope

1. [ ] [Phase 01 — `STATE.md` truth refresh + anti-drift rule](phase-01-state-truth-and-anti-drift-rule.md)
2. [ ] [Phase 02 — PR #112 reconciliation against the north star](phase-02-pr-112-north-star-reconciliation.md)
3. [ ] [Phase 03 — Gate / E2E-shard evidence audit (read-only)](phase-03-gate-e2e-shard-evidence-audit.md)
4. [ ] [Phase 04 — Founder decision: smoke shards required or advisory](phase-04-smoke-shard-gate-decision.md)
5. [ ] [Phase 05 — Worktree hygiene with an unmerged-work safety gate](phase-05-worktree-hygiene.md)

Phases 01–03 are independent and may run in any order or in parallel (they touch
disjoint files). Phase 04 depends on 03. Phase 05 is independent but its second
half waits on the #114 line closing.

## Non-goals (from the issue, kept verbatim in intent)

- Merging PR #114 — an in-flight session owns its exact-head reviews and
  merge-verify. This plan only *consumes* its triage output (Phase 04) and waits
  on its branch line (Phase 05).
- Any product feature work: roles (#116), Hermes Slice 2, mobile.
- Fixing the E2E failures themselves — that is #109 (shard 4 timeout) and #110
  (question card). This plan documents the gate's relationship to those lanes; it
  does not repair the lanes.
- Changing the smoke suite, `playwright.config.ts`, or any spec.

## DoD → phase mapping (issue #117)

Every Definition-of-Done checkbox maps to at least one phase:

| # | DoD checkbox | Phase(s) |
| - | ------------ | -------- |
| 1 | `STATE.md` refreshed and merged via a PR that itself follows the new anti-drift rule | 01 |
| 2 | Anti-drift rule present in `AGENT-WORKING-AGREEMENT.md` implementation checklist | 01 |
| 3 | PR #112 resolved (revised+merged or closed with recorded reason) | 02 |
| 4 | Gate vs E2E-shard relationship documented with evidence; founder decision recorded if a change is made | 03 (evidence + docs), 04 (decision) |
| 5 | Stale worktrees pruned | 05 |

## Named Buzz / Crew seams

The issue introduces no runtime mechanism, so there is no ACP, relay, or Nostr
seam. The seams it hangs off are the Crew-owned CI and docs surfaces:

| Concern | Seam (`path:line`) | Ownership |
| ------- | ------------------ | --------- |
| Merge gate aggregation | `.github/workflows/nuncio-crew-ci.yml:317` (`gate:`), `needs:` list at `:320` | **Crew-only** (absent from `upstream/main`) |
| Gate pass/fail rule | `desktop/scripts/check-nuncio-crew-ci-results.mjs:6` (`JOB_RELEVANCE`) | **Crew-only** |
| Advisory smoke posture | `.github/workflows/nuncio-crew-ci.yml:248-262` (`continue-on-error: true`, comment naming #36/#37) | **Crew-only** |
| Gate contract tests (the RED seam) | `desktop/src/testing/nuncio-crew-ci-contract.test.mjs:130-156` | **Crew-only** |
| Shipped-state record | `docs/crew/STATE.md` | **Crew-only** |
| Agent obligations checklist | `docs/crew/AGENT-WORKING-AGREEMENT.md:81` (`## Implementation checklist (agent)`) | **Crew-only** |
| Merge-contract doc | `docs/crew/CI.md:15-23` (job table) | **Crew-only** |
| Decisions log | `docs/crew/DECISIONS.md` (last entry D-027) | **Crew-only** |
| Temp e2e worktree creator | `desktop/tests/e2e/helpers/twoRelayHarness.ts:36` (`mkdtemp(... "buzz-ae-e2e-")`) | upstream-shared — **read-only in this plan** |

The upstream analogue of the gate is `.github/workflows/ci.yml:295-307`, which
*does* aggregate `desktop-smoke-e2e`. Crew's gate deliberately does not. That file
is inherited and **must not be edited** by this work.

## Thin-fork budget (D-001, `UPSTREAM-SYNC.md`)

**Zero upstream-file edits are planned. Expected upstream diff: 0 lines.**

Every file this plan writes was verified absent from `upstream/main`
(`git cat-file -e upstream/main:<path>` fails for each). New files land under
`docs/crew/` and `plans/`, which are Crew namespaces. If Phase 04 selects the
"make shards required" option, the edits are still Crew-only
(`nuncio-crew-ci.yml`, `check-nuncio-crew-ci-results.mjs`, and the Crew contract
test) — a `+3/-2`-scale change to the gate wiring, not an upstream touch. Should
any phase find itself wanting to edit an upstream file, stop and record why
before proceeding; nothing in the current evidence requires it.

## Generic-ACP check (D-025)

**Not applicable, explicitly.** This plan introduces no wire contract, event kind,
tag, session behavior, or assignment mechanism. Nothing here is engine-specific,
so nothing needs a "works for non-Hermes engines" proof and nothing needs a
Hermes-only label. Phase 01 must not add Hermes-only claims to `STATE.md` beyond
the ones already sourced from the Hermes track records.

## Workflow gates (D-008: Spike → RED → implementation)

| Phase | Behavior change? | Gate applied |
| ----- | ---------------- | ------------ |
| 01 | No — docs only | No spike. Verification is evidence spot-check against live repo state. |
| 02 | No — plan docs on someone else's branch, or a close | No spike. Verification is the PR's visible resolution. |
| 03 | No — read-only audit producing a written record | **This phase is the evidence gate** for Phase 04. It plays the spike role: one decision-changing question, defined PASS/FAIL/INCONCLUSIVE, reproducible citations. |
| 04 | **Yes, if the founder changes the gate** — CI merge semantics | Phase 03 evidence first, then **RED test first**: `nuncio-crew-ci-contract.test.mjs:130` currently *asserts* the advisory posture and will fail before the workflow change; it must be rewritten to assert the new contract and observed failing before the workflow edit lands. |
| 05 | No production behavior; destructive to local git state | Safety gate: content reconciliation before any branch/worktree removal (see phase). |

## Risks

| Risk | Mitigation |
| ---- | ---------- |
| **Deleting unmerged work in Phase 05.** The issue calls `.worktrees/bring-hermes-chat-into-crew` "merged-branch residue". It is not: its branch `fix/agent-attention-recovery-hardening` @ `59ab743ef` carries 6 commits with no patch-equivalent on `origin/main` **or** on PR #114's head `origin/fix/agent-attention-postmerge-audit`. | Phase 05 prunes only worktrees git itself marks `prunable`, and blocks removal of that checkout behind an explicit content reconciliation + owner confirmation. |
| `STATE.md` refreshed to a snapshot that is stale again by merge time (PRs #114, #120 are in flight). | Phase 01 writes state **as-of a stated date with named open PRs**, and re-runs the spot-check on the exact head before merge. |
| Anti-drift rule becomes unenforced prose. | Phase 01 puts it in the checklist agents already read at `AGENT-WORKING-AGREEMENT.md:81` and records it as a decision; enforcement stays review-visible (soft) by design — no CI guard is proposed, see Open questions. |
| Phase 04 makes shards required while #109/#110 are open → `main` red-walls immediately. | Phase 04's recommendation is explicitly **not** to flip while shard 4 is cancelling at the 30-minute timeout; the decision is the founder's, and Phase 03 supplies the flake numbers to make it informed. |
| Revising PR #112 (option A) means editing a branch this session does not own. | Phase 02 defaults to the close-with-reason option and requires an owner check before pushing to that branch. |

## Validation

Run `just ci` on the Phase 01/03 docs branch (docs-only paths — the path
classifier will skip desktop jobs; the gate accepts deliberate skips per
`CI.md:11-13`). Phase 04, if it changes the gate, requires the full desktop
lane plus a proven-RED-then-green contract test.

## Plan quality passes

### Validate pass — **PASS**

| Check | Result |
| ----- | ------ |
| Every issue DoD checkbox mapped to a phase | PASS — 5/5, table above |
| Issue scope respected, nothing silently dropped | PASS — problem items 1–4 map to phases 01, 02, 03+04, 05 |
| Issue non-goals honored | PASS — #114 merge, product features excluded and restated |
| Every phase has files, steps, validation, rollback | PASS |
| Buzz/Crew seams named with `path:line` | PASS — seam table |
| Thin-fork budget stated with expected diff size | PASS — 0 upstream lines |
| Generic-ACP check (D-025) stated | PASS — explicitly N/A, no mechanism introduced |
| D-020 respected (no `block/buzz` PR) | PASS — stated in header and Phase 02/04 |
| Spike → RED → implementation ordering | PASS — Phase 03 is the evidence gate; Phase 04 is RED-first |
| Anti-drift rule applied to this plan's own PRs | PASS — Phase 01 and 04 both carry `STATE.md` obligations |
| No phase depends on an unavailable input | PASS — Phase 04 and Phase 05b declare the #114 dependency |

### Red-team pass — 5 findings, 5 applied

| # | Finding | Disposition |
| - | ------- | ----------- |
| R1 | **The issue's premise for item 4 is wrong.** "Merged-branch worktree residue" — the branch has 6 commits with no equivalent on `main` or on #114. A plan that just says "prune" would have licensed data loss. | **Applied.** Phase 05 split into a safe prune (05a) and a gated removal (05b) with a reconciliation command and a default of *do nothing*. |
| R2 | **The issue's item 3 asks a question the code already answers.** "Either the Gate does not aggregate the smoke shards, or it reported on a different run" — the first is true and deliberate: `nuncio-crew-ci.yml:248-251` says so, `:258` sets `continue-on-error: true`, `:320` omits it from `gate.needs`, and `nuncio-crew-ci-contract.test.mjs:150` *asserts* the omission. Planning a discovery investigation would have burned a phase re-finding this. | **Applied.** Phase 03 is scoped to *documenting* the known-answer plus gathering flake numbers, not to discovering whether a gap exists. Its PASS criteria say so. |
| R3 | `git cherry` was the obvious safety check for Phase 05 and it is **not sufficient** — it reports `+` for `17b4353bc` even though that commit's content merged as #113 (`304173e42`), because the squash changed the patch-id. A plan that trusted it would report false unmerged work forever and the checkout would never be cleaned. | **Applied.** Phase 05b specifies a tree/content diff over the touched paths plus owner confirmation, and explicitly records that `git cherry` is inconclusive under squash-merge. |
| R4 | Phase 01 could refresh only the four items the issue lists and still leave `STATE.md` lying. Scouting found stale claims the issue does not mention: the Buzz source pin at `STATE.md:94-95` says `0.5.3 @ 3a96acea` while `docs/crew/upstream-buzz.json` says `0.5.7 @ f167818d`, and `:72` says Settings displays `v0.5.3 · Local` while `desktop/package.json:4` is `0.5.7`. | **Applied.** Phase 01 carries an explicit stale-claim inventory including these, and its validation is "no claim contradicts observable state", not "the four listed items changed". |
| R5 | Phase 04 could be read as "make the shards required" — an audit-driven change to a posture the founder's CI already deliberately chose, while the two lanes are known broken (#109 cancelling at 30 min, #110 hard-failing). That would red-wall `main` and reverse a recorded choice without asking. | **Applied.** Phase 04 presents options with trade-offs and an explicit recommendation to **keep advisory until #109/#110 close**; the founder decides. The plan never treats the flip as the default. |

## Open questions

1. **Anti-drift enforcement strength.** The issue asks for a written rule
   ("review-visible violation"). A CI guard is possible (fail a PR that changes
   `.github/workflows/nuncio-crew-*.yml` or release files without touching
   `STATE.md`) but the issue did not ask for one and it would produce false
   positives. Phase 01 ships prose + decision only. Say if a guard is wanted.
2. **Recurrence of the temp worktrees.** The 26 prunable `buzz-ae-e2e-*` entries
   come from `desktop/tests/e2e/helpers/twoRelayHarness.ts:36`, which leaks them
   on aborted runs. Fixing the harness would stop recurrence, but that is an
   upstream-shared file and outside this issue. Phase 05 prunes only; flag if the
   harness fix should become its own issue.
3. **PR #112 disposition.** Phase 02 recommends close-with-reason. Confirm before
   anyone pushes to a branch this session does not own.
