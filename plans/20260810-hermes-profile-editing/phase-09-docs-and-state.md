---
phase: 09
title: Docs truth + STATE anti-drift
status: planned
priority: high
effort: M
dependencies: [06, 07, 08]
---

# Phase 09 — Docs truth + STATE anti-drift

Issue #118 thing-to-solve 4 and DoD-4. Three shipped documents currently tell an
implementer the **opposite** of what this work does. Leaving any of them stale
would make the repo lie.

## What must change

### 1. New decision — D-028 (next free id; D-027 is the current highest)

Records that Crew becomes a **remote control for the profile**, not a second
store:

- Founder direction: *"Crew does not own the person, but Crew is where you edit
  the person."*
- **Superseded:** the presentation half of D-019 item 2 — Crew may now offer an
  editable model control for a profile-bound runtime.
- **Preserved:** the profile stays the single source of truth; Crew persists no
  model and no persona; every write goes through Hermes' own CLI/file;
  `BUZZ_ACP_MODEL` stays stripped at spawn.
- Capability-descriptor rule (D-025 continuity): behaviour is declared per
  runtime, never branched on a harness id.
- If Option B was taken in P03, record the upstream Rust edit and its approval.

### 2. `docs/crew/HERMES.md`

| Location | Change |
| -------- | ------ |
| Rule 2 (`:28-33`) | "never in Crew" → model/provider are profile-owned **and editable from Crew via write-through**; keep the `BUZZ_ACP_MODEL` strip sentence |
| Hiring § step 2 (`:79-88`) | "Leave model blank — the UI replaces the model control with 'decided by profile scout'" is now wrong; describe picking a model in Crew and the persona step |
| Hiring § step 1 | Create-in-place now includes the persona step |
| Daily operations (`:116-124`) | Add "edit the persona" alongside "change the model"; keep the C-07 next-fresh-session semantics and `!rotate` |
| Known gaps (`:197-208`) | Mark model display/edit and persona editing done; add the reset-to-default gap if P01/Q3 came back empty |
| Security caveats | Unchanged (credential fallback, local custody, shared profile state) |

### 3. `docs/crew/features/0001-hermes-first-class-runtime.md` — Slice 2 reconciliation

C-04 currently reads: *"No model field | Render | Read-only 'provided by
profile'; no picker | **Forbidden:** editable model/provider control."* That
forbidden column is now the requirement. C-04 must be **superseded with a
pointer to D-028**, not silently rewritten and not merely ticked as shipped.

Re-check the rest of the C-03…C-12 list against what is actually on main and
mark each shipped / superseded / still open. C-05, C-06, C-10, C-12, C-13, C-14,
C-15 are unchanged by this work and must stay green.

### 4. `docs/crew/STATE.md` — anti-drift (issue #117)

STATE.md is **currently stale**: its Hermes track still lists "Next gates: Slice
2 (binding/readiness/no-model UI + RED contracts), Slice 3 (upstream tier-1 PR
to block/buzz), Slice 4 (profile lifecycle UI)" — but Slices 2 and 3 shipped,
and D-020 cancelled the upstream PR entirely. Fix that alongside recording this
work. Per issue #117, any PR changing shipped state updates STATE.md **in the
same PR**.

### 5. `desktop/src/features/agents/AGENTS.md` (upstream file)

Rule 3 says a `profileArg + providerLocked + no modelEnvVar` runtime is
`ownedByProfile` and surfaces "**never an editable model control**". Rule 8 says
the model control is omitted before discovery runs. Both need updating to the
descriptor model.

**Upstream-edit justification:** this file's own closing line requires updating
it in the same PR that changes how agent configuration is modelled, rendered,
persisted, applied, or cleared. Expected delta **~15 lines**, confined to rules
3 and 8 plus the enforcing-tests list. No restructuring, no renaming.

## Docs NOT to change

- Root `AGENTS.md`, `CONTRIBUTING.md`, `ARCHITECTURE.md` — no user-facing
  workflow, command, or architecture boundary changes.
- `docs/crew/FOUNDER-PRODUCT.md` — rule 4 already requires labelling what is
  Hermes-only; this work complies rather than amends.
- `docs/crew/UPSTREAM-SYNC.md`, `IDENTITY.md` — unchanged.

## Rules

- Read each document before editing it; verify every claim against source, tests,
  or live state afterwards (`documentation-management` rule).
- No plan-phase numbers, finding codes, or audit labels in code comments,
  filenames, or commit messages — plan references belong in these phase files and
  the PR description.
- Branch name describes the product area (`agents/hermes-profile-editing`), not a
  phase number.

## Turns green

DoD-4 in full.

## Verification

```bash
# links + lint
cd desktop && pnpm check
just ci
```

Manual: re-read `HERMES.md` end to end as a new contributor and confirm no
sentence still says the model cannot be set from Crew.
