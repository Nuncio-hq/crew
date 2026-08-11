---
phase: 07
title: SOUL.md editor UI + persona at birth
status: planned
priority: high
effort: M
dependencies: [03, 05]
---

# Phase 07 — SOUL.md editor UI + persona at birth

Issue #118 thing-to-solve 2, UI half. The founder edits who the agent *is* from
inside Crew, and a profile Crew creates is born with a real persona.

## Behaviour

| State | Render |
| ----- | ------ |
| Bound profile, `SOUL.md` readable | Editor **populated with the real current content**; save writes it back |
| Read fails | Named error + the existing repair path. **Never** an empty box presented as the persona |
| Save fails | Keep the founder's unsaved text in the editor, show the classified message, do not close |
| Reset available (P01/Q3 found a source) | "Reset to Hermes default" with a confirm step, because it overwrites founder prose |
| Reset unavailable | Affordance absent; recorded as a known gap in `HERMES.md` |
| `personaDoc === "none"` (Claude Code, Codex, unknown) | Nothing renders — no empty section, no disabled control (C-15) |

**Effect timing:** like the model (C-07), a persona edit lands on the **next
fresh ACP session**, not the current turn. P01/Q4 confirms; the UI says so. This
is the residual risk recorded in `plan.md`.

## Persona at birth (DoD-2, second half)

Create-in-place currently runs `hermes profile create <name> --no-alias` and
binds, leaving the generic default `SOUL.md`. Add a **persona step** to that
flow: after a successful create, the same editor opens on the newly created
file so the founder writes a real persona before the agent ever runs. Bundled
skills are kept (D-023).

The step is skippable — skipping leaves the Hermes default, which is exactly
today's behaviour, so nothing regresses if the founder is in a hurry.

## Files

| Path | Owner | Change |
| ---- | ----- | ------ |
| `desktop/src/features/agents/ui/HermesSoulEditor.tsx` | **new, Crew-only** | populated textarea, dirty tracking, save/cancel, reset-with-confirm, effect-timing note |
| `desktop/src/features/agents/ui/HermesProfileCreateAffordance.tsx` | Crew-only | persona step after successful create |
| `desktop/src/features/agents/ui/createHermesBindingFields.tsx` | Crew-only | mount point in the create flow |
| `desktop/src/features/agents/ui/EditAgentModelAndProfileSection.tsx` | Crew-only | mount the editor when `personaDoc === "soulMd"` |
| `desktop/src/shared/api/hermesProfiles.ts` | Crew-only | consume the P05 wrappers |
| `desktop/src/features/agents/ui/AgentDefinitionDialog.tsx` / `AgentInstanceEditDialog.tsx` | **upstream, over ratchet** | **≤10 lines each**, mount only |

## Rules

- Capability-gated on `personaDoc`, never on `runtime.id`
  (`desktop/src/features/agents/AGENTS.md` rule 1).
- Byte-exact round-trip; opening and closing without editing must leave the file
  untouched (E-05).
- Layer-1 vs Layer-3 must be visually distinct: this editor is the **profile's**
  persona (shared everywhere the profile runs); the "Agent instructions" box in
  P08 is Crew's per-agent Layer-3 append. Labelling them the same way is the
  confusion the issue is trying to end.
- rem-based text tokens only (`pnpm check:px-text`); new markup lives in
  Crew-owned files, not the two over-ratchet dialogs.
- Playwright: `waitForAnimations(page)` before any screenshot; scope shots with
  `locator.screenshot()` and verify hashes are distinct before posting.

## Turns green

E-16.

## Verification

```bash
just desktop-typecheck
cd desktop && pnpm check && pnpm check:px-text && pnpm check:file-sizes
cd desktop && pnpm test:e2e:smoke
```

Manual: open `scout`'s persona, confirm it matches disk, edit and save, confirm
the file changed; create a throwaway profile from Crew and confirm the persona
step writes a real `SOUL.md` before first run.
