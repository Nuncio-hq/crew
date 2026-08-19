# Feature 0002 — Role-first CoS (call by name)

- **Status:** PROPOSED
- **Date:** 2026-08-19
- **Issues:** #230 (call by name), #231 (role-first discussion), #232 (CoS intake)
- **Founder locks (2026-08-19 brainstorm):**
  - Role-first: channel roles are the work wall; hide/demote Org chart UI;
    keep `KIND_ORG_ROSTER` / budget dormant until needed
  - CoS calls specialists **by name** (not by role label) — real-company native
  - Founder does **not** Wake agents; CoS does
  - CoS may prototype small; feature work → named specialist (e.g. Dev)
  - Factory-as-code stays discussion
  - Mobile later; Flutter first (RN not default)
  - This session: planning + issues only — no production implementation

## Stories

1. Oscar @CoS once. CoS calls `@Dev` by name when implementation is needed.
   Dev wakes via existing mention path (tool may wrap it). Reports stay in
   the room. Oscar’s Inbox is Need you, not a Wake console.
2. In each office channel, roles say what a named person may do (intake /
   code / …). Wrong-role feature work refuses or escalates.

## Decisions this feature must not reopen

- Channel-first, no board-as-home (D-037)
- No Workbench place (D-065)
- Thin fork; Buzz contracts first (D-001, D-025)
- Flutter mobile unless a later decision supersedes FOUNDER-PRODUCT
- Call-by-role APIs (explicitly rejected)

## Spikes already run

| Spike | Question | Verdict | How we use it now |
| ----- | -------- | ------- | ----------------- |
| [0052](../spikes/0052-inbox-sleeping-continue-without-new-kind.md) | Can Inbox show sleep without a new kind? | PASS | Evidence only. **Not** the founder Wake slice. |
| [0053](../spikes/0053-cos-handoff-reuses-crew-handoff.md) | Can CoS delegate without a new kind? | PASS | Handoff tag may back named assignment; day-one call is by name. |

## Slices (draft — not approved to implement)

1. **Agent call-by-name tool** (#230) — CoS → named agent → wake on that
   thread. Spike: is mention enough, or MCP/`buzz` wrapper?
2. **Role-first decision** (#231) — hide Org UI vs keep protocol; where
   budgets live; rewrite or drop officer-loop-on-tree.
3. **CoS intake on one channel** (#232) — roles assigned; prompt rules;
   Oscar never @Dev for feature-sized work.

Implementation only after spike → RED → founder approves that slice
(D-008).

## Out of scope

- Founder Inbox Wake button as primary recovery
- Warp `factory.yaml` as live truth
- Deleting 30680 before #231 decides
- #102 / #151 mission machine
- React Native client
