---
phase: 06
title: Offboarding archive, restore, and permanent-delete UI
status: planned
priority: P0
effort: L (3 d)
dependencies: ["04", "05"]
---

# Phase 06 — Offboarding archive, restore, and permanent-delete UI

## Outcome

The destructive offboard branch becomes archive. The owner can bring an
archived employee back and re-bind them. Permanent destruction exists,
but only for something already archived, and only after typing the
profile name.

DoD coverage: #3 (UI half), #4 (UI half — advisory).

## Design constraints inherited from the plan

- **DD-4:** the running-agent guard is authoritative in Rust (phase 04).
  The UI disable is **advisory** — it must state the reason and offer a
  path to stop the agent, and it must handle a backend refusal
  gracefully even when the UI believed the action was allowed.
- Permanent delete is **never** offered on a live profile. Only on an
  archive. This is the issue's explicit requirement and the guardrail
  that makes archive-by-default safe.
- Keep the existing keep-vs-destructive shape and its test ids
  (`data-testid="hermes-profile-offboard-{keep,delete}"`); **keep**
  remains the default (C-13). Archive replaces the *destructive* branch;
  it does not become the new default.
- Every profile name entering a destructive path is validated and
  `default`-rejected (`hermes_profile.rs:13,19`) — client-side is
  explanatory, the server is the authority (HERMES.md rule 5 pattern).

## Seams

| Seam | Use |
| ---- | --- |
| `features/agents/ui/usePersonaActions.ts:297-317` | The irreversible `deleteHermesProfile` loop being replaced (~+15 / -20) |
| `features/agents/ui/PersonaDeleteDialog.tsx` | Dialog shell stays upstream; mount Crew-owned fields (~+10) |
| `features/agents/ui/HermesProfileOffboardFields.tsx` (Crew-owned) | The radio set gains the archive option and the size estimate |
| `shared/api/hermesProfiles.ts` | Phase-04 invoke wrappers |
| Existing binding UI / profile picker (`EditAgentModelAndProfileSection.tsx`) | Re-bind after restore — reuse, do not build a second binding surface |
| `hermesProfileDeleteCommandLine(name)` (shown in offboard fields) | The auditable-command-line pattern (P-6) to mirror for archive |

## Work

1. **RED tests** first (DD-7):
   - Offboard dialog: keep is default; the destructive option is
     archive; permanent delete is absent for a live profile.
   - Size estimate renders before the action.
   - Archive disabled with a stated reason while the agent runs;
     enabled after stop.
   - A backend refusal (guard, collision, invalid name) surfaces its
     named message rather than a generic failure.
   - Restore picker lists archives with manifest info; restore onto a
     colliding live name is blocked with a clear message.
   - Permanent delete requires the exact typed profile name; a
     near-miss does not enable the action.

2. **Offboard fields**: add the archive option to the Crew-owned
   `HermesProfileOffboardFields.tsx`, with the size estimate from
   phase 04, the optional free-text offboard reason (written to the
   manifest), and the auditable command/operation line.

3. **`usePersonaActions.ts`** (upstream): replace the
   `deleteHermesProfile` branch at `:297-317` with a call into a
   Crew-owned hook that invokes the archive command. Keep the edit
   minimal — the logic lives in the Crew-owned hook.

4. **Restore surface**: a Crew-owned view listing archives (name,
   timestamp, bound agent, reason, size). Restore → on success, offer
   re-bind through the *existing* binding UI. Collision message is the
   backend's named result, surfaced verbatim in intent.

5. **Permanent delete**: on an archive row only, behind
   type-the-profile-name confirmation, passing the token to the backend
   (which is the real gate).

6. **`docs/crew/STATE.md`** updated in this PR (#117).

## Files

- **Modify (upstream, justified):** `usePersonaActions.ts`,
  `PersonaDeleteDialog.tsx`
- **Modify (Crew-owned):** `HermesProfileOffboardFields.tsx`
- **Create (Crew-owned):** archive hook, restore/archive list view,
  permanent-delete confirmation component + tests
- **Must not touch:** the create flow, occupancy checks, owner-only /
  local invariants (issue non-goals); the phase-04 backend contracts

## Validation

- RED tests green, including the guard-refusal and near-miss-token
  cases.
- `pnpm exec tsc --noEmit` clean; biome clean; `pnpm check:px-text`
  passes.
- `node desktop/scripts/check-file-sizes.mjs` passes — if
  `HermesProfileOffboardFields.tsx` or a dialog grows, split by
  responsibility (D-022), never raise `MAX_LINES`.
- Playwright specs (mock bridge) for offboard-with-archive, restore
  picker, collision block, and the type-name gate. Build with
  `pnpm build:e2e`; kill port 4173 first if a stale preview server is
  running (`reuseExistingServer: true` will otherwise serve old code).
  `page.addInitScript` before `installMockBridge(page)`; call
  `waitForAnimations(page)` before every screenshot.
- Distinct-state screenshots: `locator.screenshot()` per state,
  `shasum -a 256` all-unique gate before posting via
  `scripts/post-screenshots.sh`.
- `just ci` green.

## Risk and rollback

- **Risk:** the owner reads "archive" as "delete" and expects space
  freed. Mitigation: copy states plainly that the profile is filed and
  restorable, and shows the archive size.
- **Risk:** UI and backend disagree about liveness, so a disabled
  button hides a working action or an enabled one gets refused.
  Mitigation: the backend is authoritative (DD-4); the UI surfaces the
  refusal rather than swallowing it.
- **Risk:** restore-then-rebind leaves an agent bound to a name that
  does not exist if the rebind step is abandoned. Mitigation: restore
  completes independently of rebind; the resulting unbound state is
  exactly the existing `missing` readiness class from phase 02, already
  repairable from the nudge.
- **Rollback:** revert the two upstream edits — offboarding returns to
  keep-vs-delete. Phase-04 commands become unreferenced but harmless.

## PR

Branch `agents/profile-offboard-archive`. Target `Nuncio-hq/crew`
(D-020). `git commit -s`. PR body carries the distinct-hash screenshot
set for offboard / restore / permanent-delete states.
