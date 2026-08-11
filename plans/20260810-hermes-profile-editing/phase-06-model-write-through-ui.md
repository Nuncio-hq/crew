---
phase: 06
title: Model write-through UI
status: planned
priority: high
effort: M
dependencies: [03, 04]
---

# Phase 06 — Model write-through UI

Issue #118 thing-to-solve 1, UI half. The founder sees and changes the model
inside Crew, and is told plainly what that means.

## Copy the issue requires

> **This model belongs to profile `<name>` — changing it here changes it
> everywhere the profile runs, not just in Crew.**

This note is mandatory whenever `modelSource === "profileWriteThrough"`. It is
the honesty requirement from `FOUNDER-PRODUCT.md` rule 4 in UI form: the founder
must never think Crew is holding a private setting.

## Behaviour

| State | Render |
| ----- | ------ |
| Bound profile, model readable | Editable model + provider control seeded from the profile, plus the shared-everywhere note |
| Read fails (`BinaryMissing` / `DoesNotExist`) | Fall back to the existing informational row and the existing repair path — never an empty editable field implying "no model" |
| Write succeeds | Re-read from the profile and display the read-back value; invalidate the query |
| Write fails | Keep the previous value, show the classified message from P04, leave the control recoverable (red-team R-4) |
| Non-profile runtime | Exactly today's behaviour (C-15) |
| Discovery not yet run | Same deferral rule as today (`desktop/src/features/agents/AGENTS.md` rule 8) — do not flash a control that will disappear |

**Effect timing:** a model change lands on the agent's **next fresh ACP
session** (C-07, `HERMES.md:116-120`). `!rotate` forces one. The UI must say so
rather than implying an immediate switch.

## New-profile flow (DoD-1, second half)

Create-in-place (`HermesProfileCreateAffordance.tsx`) currently runs
`hermes profile create <name> --no-alias` and binds. After a successful create,
offer model selection through the same P04 write path — one explicit step, still
auditable, still Crew-owned. Bundled skills stay (D-023); no `--no-skills`.

## Files

| Path | Owner | Change |
| ---- | ----- | ------ |
| `desktop/src/features/agents/ui/HermesProfileModelField.tsx` | **new, Crew-only** | editable control + note + error/read-back states |
| `desktop/src/features/agents/ui/EditAgentModelAndProfileSection.tsx` (127 lines, Crew-only, S-12) | Crew-only | branch at `:71` renders the new field instead of `ProfileOwnedModelRow` when `modelSource === "profileWriteThrough"` |
| `desktop/src/features/agents/ui/HermesProfileBindingFields.tsx` (335 lines, Crew-only, S-13) | Crew-only | keep `ProfileOwnedModelRow` at `:40` as the read-failure fallback; retire it as the default |
| `desktop/src/features/agents/lib/hermesProfileBinding.ts` | Crew-only | replace `profileOwnedModelLabel` (`:170`) copy with the write-through note builder |
| `desktop/src/features/agents/ui/createHermesBindingFields.tsx` | Crew-only | post-create model step |
| `desktop/src/features/agents/ui/AgentDefinitionDialog.tsx` (1016 lines, **upstream, over ratchet**) | upstream | **≤10 lines** — mount the Crew-owned child only |
| `desktop/src/features/agents/ui/AgentInstanceEditDialog.tsx` (1224 lines, **upstream, over ratchet**) | upstream | **≤10 lines** — same |

**File-size rule:** both dialogs already exceed `MAX_LINES = 1000`
(`desktop/scripts/check-file-sizes.mjs`). All markup goes into Crew-owned
children. Never raise the limit; never add an override (D-022).

## Rules that still bind this phase

- No `runtime.id === "hermes"` in render code — read the P03 descriptor
  (`desktop/src/features/agents/AGENTS.md` rule 1).
- Crew stores nothing (red-team R-3). The query cache is invalidated on write and
  on dialog open; it is never the source of truth.
- `BUZZ_ACP_MODEL` stays stripped at spawn for profile-locked runtimes
  (C-05/C-06, issue non-goal). Editing a model in Crew must not reintroduce it.
- Text sizing: rem-based Tailwind tokens only, no arbitrary px/rem literals
  (`pnpm check:px-text`).

## Turns green

E-09 (UI half), E-15; keeps E-13 green.

## Verification

```bash
just desktop-typecheck
cd desktop && pnpm check && pnpm check:px-text && pnpm check:file-sizes
cd desktop && pnpm test:e2e:smoke
```
