---
phase: 02
title: PR #112 reconciliation against the north star
status: pending
priority: high
effort: S
dependencies: []
---

# Phase 02 — PR #112 reconciliation against the north star

- **Issue:** #117 — problem item 2; DoD checkbox 3
- **PR scope:** either a revision commit on `docs/plans-open-issues`, or a close
  with a recorded reason. No code either way.
- **Target repo:** `Nuncio-hq/crew` only (D-020).

## Context

PR [#112](https://github.com/Nuncio-hq/crew/pull/112) "docs(plans): execution
plans for the six open issues" was opened 2026-08-09T03:59Z. It adds seven
docs-only files:

```
plans/20260809-0355-open-issues-sequencing/plan.md      (index, 78 lines)
plans/20260809-0400-e2e-shard4-revival/plan.md          (#109)
plans/20260809-0405-channel-question-card/plan.md       (#110)
plans/20260809-0410-file-size-ratchet-upstream-files/plan.md (#111)
plans/20260809-0415-channel-first-missions/plan.md      (#102)
plans/20260809-0420-hermes-first-class-operations/plan.md (#104)
plans/20260809-0425-agent-attention-recovery/plan.md    (#105)
```

It predates the north star: `FOUNDER-PRODUCT.md` and D-025/D-026/D-027 landed
2026-08-10 via #115 (`06107122b`), and issue #116 (agent roles) did not exist when
#112 was written. Its sequencing index is the risk — merging a *sequencing
authority* that predates the locked product direction commits Crew to an ordering
nobody re-checked.

Note what #112 already got right and do not discard it: it carries a real bisect
for #110 (`AppShell.tsx` → `useLiveHomeFeedActions` subscription races the test's
readiness gate) and corrects the issue's own stated cause. That evidence is
worth keeping wherever it ends up.

## The decision

Two options. **Recommended: B.**

| | A — revise and merge | B — close with recorded reason |
| - | -------------------- | ------------------------------ |
| Work | push a revision commit reconciling the index with `FOUNDER-PRODUCT.md`, D-025–D-027, and #116/#120 | comment on #112 linking the superseding issues, close, keep nothing on disk |
| Pro | preserves the #110 bisect and the per-issue plans in-tree | no stale sequencing authority; each issue keeps its own plan as it is planned |
| Con | requires pushing to a branch this session does not own; the index needs re-deciding against a product direction that changed under it; #105 and #108 have since merged, so parts are already historical | loses the #110 bisect unless it is copied into #110 first |
| Thin-fork / drift | a sequencing index is a stateful record, not evergreen authority (`documentation-management` rule) — keeping it invites future agents to treat it as law | matches how the other five issues are being planned today (one plan dir per issue, at planning time) |

**Recommendation rationale:** #112's own body says #102/#104/#105 keep their specs
in the issue bodies and the plan files add only status, coupling, and ordering.
Ordering is exactly the part the north star and #116 invalidated, and status is
already stale (#105/PR #108 merged, #113 merged). What survives is the #110
bisect — which belongs on #110 regardless.

## Steps

1. Re-read #112's index (`plans/20260809-0355-open-issues-sequencing/plan.md`)
   against `docs/crew/FOUNDER-PRODUCT.md`, D-025/D-026/D-027, and issues
   #116/#121. Write down each ordering claim that the north star changed.
2. **Before touching the branch**, confirm ownership — #112 was authored by a
   different session. If it has a live owner, hand them this phase's finding
   rather than pushing.
3. If B: copy the #110 root-cause bisect table into a comment on issue #110 so the
   evidence survives, then close #112 with a comment naming the superseding
   issues (#116, #117, #121) and stating the reason: *sequencing index predates
   the locked founder product direction; per-issue plans are being produced at
   planning time instead*.
4. If A: push one revision commit that rewrites the index against the north star
   and drops the already-shipped entries (#105/#108, #113), then merge through
   `NuncioCrew Gate`.
5. Record which option happened, and where, in this phase file's status line.

## Validation

- The resolution is **visible on the PR itself** (issue #117's stated bar): either
  a merge commit, or a closing comment that names the superseding issues.
- If B: the #110 bisect is present on issue #110 before #112 closes.
- No product-code change in either option; no `block/buzz` PR (D-020).

## Risk and rollback

- **Risk:** pushing to a branch owned by another session mid-flight. Mitigation:
  step 2's ownership check is a hard gate.
- **Risk:** closing loses evidence. Mitigation: step 3 preserves the bisect first.
- **Rollback:** a closed PR can be reopened; a revision commit can be reverted on
  the branch before merge.
