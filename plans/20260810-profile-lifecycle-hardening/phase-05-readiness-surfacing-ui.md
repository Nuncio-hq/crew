---
phase: 05
title: Readiness surfacing — agent card and edit dialog
status: planned
priority: P1
effort: M (2 d)
dependencies: ["02"]
---

# Phase 05 — Readiness surfacing: agent card and edit dialog

## Outcome

The owner can see profile health at a glance in AgentsView, without
opening a dialog — and can see the detail and the repair when they do.
`auth-unknown` reads as a known limit, not a fault.

DoD coverage: #1 (card + dialog half), #5 (display half).

## Design constraints inherited from the plan

- **DD-2:** consume the per-agent projection from phase 02. No frontend
  rival table, no `runtime.id` check in any component
  (`features/agents/AGENTS.md` rule 1).
- **AGENTS.md rule 3:** named reasons, never booleans. The specific
  target is `managedAgentRuntimeStatus.ts:12`:
  `if (!runtime.localSetup) return "Needs setup on this device"` — the
  boolean the issue is asking to replace.
- **DD-1:** `auth-unknown` renders as neutral/informational and never as
  an error state. Every healthy Hermes agent is `auth-unknown` today; a
  warning treatment would train the owner to ignore the badge (plan
  risk table).
- Text sizing: rem tokens only, never px (`AGENTS.md` § Text sizing).
  Meta text uses `text-2xs` / `text-3xs`; no arbitrary literals — the CI
  guard `pnpm check:px-text` fails on px *and* rem literals.

## Seams

| Seam | Use |
| ---- | --- |
| `features/agents/managedAgentRuntimeStatus.ts:12` | Replace the `localSetup` boolean read with the named state (~+20 / -4) |
| `features/agents/ui/AgentStatusBadge.tsx` | Badge variant per state (~+12) |
| `features/agents/ui/ManagedAgentRow.tsx` | Render the badge on the card (~+8) |
| `features/agents/ui/AgentsView.tsx:~218` (`<UnifiedAgentsSection agents={agents.managedAgents} …>`), profiles collected `:398-416` | The card list — read-only if the badge composes into `ManagedAgentRow` |
| `shared/ui/config-nudge-attachment.tsx:43,139,162,421` | Two new render arms delegating to Crew-owned rows (~+20) |
| `shared/ui/HermesProfileOrphanRepairRow.tsx` | The pattern for the new repair rows; new rows are Crew-owned siblings |
| `features/agents/ui/EditAgentModelAndProfileSection.tsx` (Crew-owned) | Dialog detail lives here |

## Work

1. **RED tests** first (DD-7): component/unit tests asserting each of
   the five states renders its own copy; `auth-unknown` renders
   informational, not error; no component references `runtime.id`.

2. **Named status projection**: replace the boolean read in
   `managedAgentRuntimeStatus.ts` with the phase-02 state, mapping each
   to copy that names the fault *and* the repair.

3. **Card badge**: variant in `AgentStatusBadge.tsx`, rendered by
   `ManagedAgentRow.tsx`. Glanceable — state distinguishable without
   opening anything. Keep `AgentsView.tsx` untouched if the badge
   composes into the row (preferred; smaller upstream delta).

4. **Dialog detail** in the Crew-owned edit section: full state, copy,
   and the repair affordance.

5. **Nudge repair rows**: two new Crew-owned row components beside
   `HermesProfileOrphanRepairRow.tsx` — one for broken config, one for
   missing binary — wired by two additive arms in
   `config-nudge-attachment.tsx`. Each states the fault and offers the
   repair without requiring a terminal (plan R7).

6. **`auth-unknown` copy**: "auth not verifiable", linking the
   spike-0010 ask, per the issue's explicit wording — never green, never
   red.

7. **`docs/crew/STATE.md`** updated in this PR (#117).

## Files

- **Modify (upstream, justified):** `managedAgentRuntimeStatus.ts`,
  `AgentStatusBadge.tsx`, `ManagedAgentRow.tsx`,
  `shared/ui/config-nudge-attachment.tsx`
- **Create (Crew-owned):** two nudge repair row components + tests
- **Modify (Crew-owned):** `EditAgentModelAndProfileSection.tsx`
- **Must not touch:** `AcpRuntimeCatalogEntry` / catalog,
  `lib/agentConfigCore.ts` (DD-2); `usePersonaActions.ts` and the delete
  dialogs (phase 06)

## Validation

- RED tests green; the "no `runtime.id` in components" assertion holds.
- `pnpm exec tsc --noEmit` clean; biome clean.
- `pnpm check:px-text` passes — no new px or arbitrary rem text sizes.
- `node desktop/scripts/check-file-sizes.mjs` passes (D-022 if near).
- Playwright smoke for the card states, built with `pnpm build:e2e`
  (never plain `pnpm run build` — the mock bridge is compiled in only
  for `--mode e2e`; a plain build fails every mock spec with
  `Cannot read properties of undefined (reading 'invoke')`).
- Screenshot states are **distinct**: scope each shot with
  `locator.screenshot()` and gate on
  `shasum -a 256 test-results/<dir>/*.png` — every hash unique before
  posting. Post via `scripts/post-screenshots.sh`; delete superseded
  comments.
- `just ci` green.

## Risk and rollback

- **Risk:** badge noise — five states on every card is visual clutter.
  Mitigation: `ready` renders no badge; only non-`ready` states draw
  attention. `auth-unknown` is detail-level, not a card badge, unless
  the phase-01/product read says otherwise.
- **Risk:** stale state on the card if the projection does not refresh.
  Mitigation: the phase-03 re-evaluation trigger drives it; verify the
  card clears after a repair without an app restart.
- **Rollback:** revert the four upstream edits; the boolean status
  returns. New Crew-owned rows become unreferenced.

## PR

Branch `agents/profile-readiness-ui`. Target `Nuncio-hq/crew` (D-020).
`git commit -s`. PR body carries the distinct-hash screenshot set for
the card states.
