## Issue-to-Plan Handoff — #117

Plan: `plans/20260810-docs-truth-gate-audit/plan.md` (5 phases, docs + CI-posture
only). Validate: **PASS**. Red-team: **5 findings, all applied**.

### Phases

| # | Title | Effort | Depends on | DoD |
| - | ----- | ------ | ---------- | --- |
| 01 | `STATE.md` truth refresh + anti-drift rule | M | — | 1, 2 |
| 02 | PR #112 reconciliation against the north star | S | — | 3 |
| 03 | Gate / E2E-shard evidence audit (read-only) | M | — | 4 (evidence) |
| 04 | Founder decision: smoke shards required or advisory | S | 03 (+ #114 triage, soft) | 4 (decision) |
| 05 | Worktree hygiene with an unmerged-work safety gate | S | — | 5 |

01–03 are independent and touch disjoint files. All 5 DoD checkboxes are mapped.

### Key design decisions

- **Item 3 is already answered by the code — no investigation phase needed.** The
  Gate genuinely ignores the smoke shards, on purpose: `nuncio-crew-ci.yml:258`
  sets `continue-on-error: true`, `:320` omits the job from `gate.needs`, and
  `nuncio-crew-ci-contract.test.mjs:150` *asserts* that omission. The in-line
  comment at `:248-251` names #36/#37. So PR #114's SUCCESS-over-red-shards is
  designed behavior, not a mis-reported run. The real defect is that
  `docs/crew/CI.md:15-23` never mentions the smoke job, so a green gate reads as
  "E2E passed". Phase 03 documents it; Phase 04 asks whether to change it.
- **Recommendation: keep the shards advisory until #109 and #110 close.** Making
  a known-broken lane required red-walls every desktop PR without fixing a test.
  The founder decides; the plan does not flip the gate.
- **Item 4's premise is wrong and the plan corrects it.**
  `.worktrees/bring-hermes-chat-into-crew` is *not* merged residue: its branch has
  6 commits not on `main` and not on PR #114's head. `git cherry` is unusable here
  (squash-merge rewrites patch-ids, so it flags even the commit that merged as
  #113). Phase 05 splits into a safe `git worktree prune` of the 24 prunable
  `buzz-ae-e2e-*` entries, and a **gated** removal that defaults to no action.
- **Scope kept honest:** Phase 01 also fixes stale claims the issue did not list —
  the Buzz pin at `STATE.md:94-95` says `0.5.3` while `upstream-buzz.json` says
  `0.5.7`, and `:72` claims Settings shows `v0.5.3` while the app is `0.5.7`.
- **PR #112: recommend close-with-reason**, after copying its #110 root-cause
  bisect onto issue #110 so the evidence survives.

### Seams (all Crew-owned — 0 upstream lines expected)

- Gate aggregation: `.github/workflows/nuncio-crew-ci.yml:317` / `:320`
- Gate pass/fail rule: `desktop/scripts/check-nuncio-crew-ci-results.mjs:6`
- RED contract seam: `desktop/src/testing/nuncio-crew-ci-contract.test.mjs:130-156`
- Agent obligations: `docs/crew/AGENT-WORKING-AGREEMENT.md:81`
- Merge-contract doc: `docs/crew/CI.md:15-23`
- Temp-worktree source: `desktop/tests/e2e/helpers/twoRelayHarness.ts:36` (read-only)

Upstream's `.github/workflows/ci.yml:295` *does* aggregate smoke — inherited,
read-only, not Crew's to change. All PRs target `Nuncio-hq/crew` (D-020).
D-025 generic-ACP check: **N/A, explicitly** — no wire contract, event kind, or
engine-specific behavior is introduced.

### Open questions

1. Should the anti-drift rule get a CI guard, or stay review-visible prose only?
   (Issue asks for a rule; plan ships prose + a decision entry.)
2. The `buzz-ae-e2e-*` leak recurs from the e2e harness. Prune only, or file a
   separate issue to fix the harness cleanup?
3. Confirm PR #112 should be closed rather than revised — it belongs to another
   session's branch.
