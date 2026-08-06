# Hermes profile picker (pick existing, create new)

- **Status:** Implemented — PR pending
- **Date:** 2026-08-06
- **Issue:** [#78](https://github.com/Nuncio-hq/crew/issues/78)
- **Parent feature:** [#51](https://github.com/Nuncio-hq/crew/issues/51) /
  [`docs/crew/features/0001-hermes-first-class-runtime.md`](../../docs/crew/features/0001-hermes-first-class-runtime.md)
- **Parent plan:** [`../20260805-1330-hermes-first-class-runtime/plan.md`](../20260805-1330-hermes-first-class-runtime/plan.md)
  — this is **Phase 04** of that work
- **Decisions:** D-019 (binding rules), D-020 (Crew not block/buzz), D-023
  (bundled skills on create) — no new decision expected
- **Branch (suggested):** `feat/hermes-profile-picker`

## Manager-visible outcome

When hiring or editing a Hermes agent, Oscar **sees profiles already on
disk** (`scout`, `builder`, …), picks one, or types a new name and
explicitly creates it. He no longer has to remember names or fall back to
per-profile custom harness JSONs in the engine dropdown.

## Why now

Phases 02–03 shipped binding + create-in-place + `list_hermes_profiles`,
but `HermesProfileField` is still free-text only. Feature story **S-2.1**
(“pick existing **or** create in place”) is half done. The missing pick
path is what made legacy `Hermes (scout)` / `Hermes (builder)` custom
harnesses feel necessary.

## Spike evidence (no new spike)

Uncertainty was “can Crew list profiles headlessly?” — answered in Phase
03 / spike 0011:

| Fact | Evidence |
| ---- | -------- |
| List is a directory read of `~/.hermes/profiles` (or `$HERMES_HOME/profiles`) | `hermes_profile_lifecycle::list_profiles` |
| IPC + TS wrapper exist | `list_hermes_profiles` / `listHermesProfiles()` |
| `default` rejected for bind/create/delete | D-019 + service layer |
| Create is explicit, auditable | `HermesProfileCreateAffordance` + D-019 item 6 |

**No new feasibility spike.** Remaining work is UI composition + occupancy
join + tests. If list IPC regresses, Phase 1 degrades to free-text (fail
open), not a product redesign.

## Scope

| # | Phase | Ships | Depends |
| - | ----- | ----- | ------- |
| 1 | [Combobox pick-or-create (04a)](phase-01-combobox-pick-or-create.md) | List + select + keep create-in-place | — |
| 2 | [Occupancy badges (04b)](phase-02-occupancy-badges.md) | free / bound·@Name early feedback (C-10 UX) | Phase 1 |

**Deferred (not this plan unless expanded):**

- 04c — suggest profile name from agent display name
- 05 — reverse “Hire from profile” panel
- 06 — live ACP model in “decided by profile” row (#73 follow-up)

## Locked boundaries (do not reopen)

1. One Crew agent ↔ one named Hermes profile; never bind `default`.
2. No per-profile entries in the **Agent harness** dropdown.
3. Create profile only via explicit manager action (never silent on Save).
4. No `runtime.id === "hermes"` in components — capability via field model /
   `profileArg`.
5. Prefer existing directory IPC over shelling `hermes profile list`.
6. Server duplicate-bind reject remains authoritative (C-10); UI badge is
   advisory.
7. Thin fork: Crew-owned desktop files only; no upstream `block/buzz` PR
   (D-020).

## Non-goals

- Spawn / readiness / model-ownership changes
- Custom harness migration tooling
- Profile metadata beyond name + occupancy (model/provider inside picker)
- Auth probe (still blocked on Hermes-side ask, spike 0010)

## Contracts (issue #78 → tests)

| ID | Scenario | Expected | Forbidden |
| -- | -------- | -------- | --------- |
| P-01 | Disk has `scout`, `builder` | Both appear in profile control without full manual recall | Empty free-text only |
| P-02 | Select `scout`, unbound | Value binds; create/save succeeds | Spawn without `-p scout` (regression of #60) |
| P-03 | List options | `default` never selectable | `default` in list |
| P-04 | Typed name not on disk | Create affordance remains; create refreshes list | Silent create on Save |
| P-05 | List IPC throws / empty home | Free-text + validation still work | Dialog bricked |
| P-06 | (Phase 2) Profile bound to other agent on relay | Badge shows bound agent; save still server-rejected if forced | Silent double-bind |
| P-07 | Goose / non-`profileArg` runtime | No profile control (existing) | Hermes-only UI leak |

## Files (expected touch set)

**Prefer new / existing Crew desktop surfaces:**

| Area | Path |
| ---- | ---- |
| Field UI | `desktop/src/features/agents/ui/HermesProfileBindingFields.tsx` |
| Create affordance | `desktop/src/features/agents/ui/HermesProfileCreateAffordance.tsx` (wire refresh) |
| Pure option builders | `desktop/src/features/agents/lib/hermesProfileBinding.ts` (+ tests) |
| Optional hook | `desktop/src/features/agents/lib/useHermesProfilesQuery.ts` (or colocated) |
| API (reuse) | `desktop/src/shared/api/hermesProfiles.ts` |
| E2E mock | `desktop/src/testing/e2eBridge.ts` (`list_hermes_profiles` already mocked) |
| E2E | `desktop/tests/e2e/hermes-profile-binding.spec.ts` |
| Docs | `docs/crew/HERMES.md` hire section |
| Parent plan status | `plans/20260805-1330-hermes-first-class-runtime/plan.md` |

**Rust:** none expected for 04a/04b (`list_profiles` already shipped).

**Upstream files:** none.

## Rollback

Revert the PR(s). Field returns to free-text Input; lifecycle IPC and
binding storage unchanged. No data migration.

## Verification commands

```bash
# Unit / contract
cd desktop && NODE_ENV= pnpm test -- hermesProfile
# or targeted:
node --test desktop/src/features/agents/lib/hermesProfileBinding*.test.mjs

# Type + lint surface
just desktop-typecheck
cd desktop && pnpm check && pnpm check:px-text && pnpm check:file-sizes

# E2E (mock bridge)
cd desktop && pnpm exec playwright test --project=smoke tests/e2e/hermes-profile-binding.spec.ts

# Optional full desktop gate if PR is larger
just desktop-tauri-test   # only if Rust touched (should be N/A)
```

Manual (manager machine with real profiles):

1. Create agent → Hermes Agent → see `scout` / `builder` → pick `scout`.
2. Type `research` → Create profile → selected + reappears in list.
3. Second agent: `scout` shows bound (Phase 2); save errors if forced.
4. `default` absent; typing `default` still validation-blocked.

## Unresolved questions for manager

1. **Phase 2 in same PR or follow-up?** Recommendation: Phase 1 alone
   closes the main pain; Phase 2 can ride along if small, else separate PR
   on same branch series.
2. **Selecting a bound profile:** allow select + disable Save with inline
   reason, or prevent select entirely? Recommendation: **allow select,
   block save** with copy matching server C-10 (edit-self exception).
3. **Combobox vs select-only dropdown:** free-text must remain for create
   path → **combobox** (ChannelCombobox pattern), not select-only.

## Approval checkpoint

No production implementation until this plan is approved. After approval:

1. RED contract tests (option builder + E2E pick path)
2. Phase 1 implementation → green
3. Phase 2 (if in scope) → green
4. Docs + parent plan checkbox update
5. PR → Nuncio-hq/crew only (D-020)
