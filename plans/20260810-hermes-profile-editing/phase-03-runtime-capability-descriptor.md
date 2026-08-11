---
phase: 03
title: Runtime capability descriptor
status: planned
priority: high
effort: S
dependencies: [01, 02]
---

# Phase 03 — Runtime capability descriptor

Issue #118 § Design constraint. Gives every runtime a declared
`{ modelSource, personaDoc, layer3 }` so P06/P07 render from a capability, not
from a harness id. This is the D-025 obligation and the
`desktop/src/features/agents/AGENTS.md` rule-1 obligation in one place.

## Deliverable

```text
Hermes               : { modelSource: "profileWriteThrough", personaDoc: "soulMd", layer3: "append" }
Claude Code / Codex  : { modelSource: "adapterSetting",      personaDoc: "none",   layer3: "append" }
Unknown / custom     : { modelSource: "adapterSetting",      personaDoc: "none",   layer3: "append" }
```

## Design — Option A (default, zero upstream edits)

Derive the descriptor **once**, at the Crew-owned catalog projection boundary
`desktop/src/shared/api/fromRawAcpRuntimeCatalog.ts:71` (S-8), from facts the
Rust catalog already projects through `types/crew.rs:7` (S-6):

| Fact | Source | `path:line` |
| ---- | ------ | ----------- |
| `profileArg` | `KnownAcpRuntime::profile_arg` | `discovery/runtime_metadata.rs:70` |
| `providerLocked` | `KnownAcpRuntime::provider_locked` | `discovery/runtime_metadata.rs:43` |
| `modelEnvVar` | `KnownAcpRuntime::model_env_var` | `discovery/runtime_metadata.rs:41` |

`profileArg && providerLocked && !modelEnvVar` is exactly the predicate
`runtimeOwnsModelViaProfile` already uses
(`desktop/src/features/agents/lib/hermesProfileBinding.ts:24`). The descriptor
renames what that predicate means rather than inventing a parallel truth: the
same shape now yields `modelSource: "profileWriteThrough"` instead of "render a
read-only row".

**The one fact that cannot be derived** is the persona filename. `SOUL.md` is
named once, as data, in the new Crew-only capability module — never as a branch
inside a component. That limitation is stated in `plan.md` and is the trigger
for Option B.

### Option B — escalation only

Declare `persona_doc` on `KnownAcpRuntime` and project it through
`discovery.rs:1273` + `types/crew.rs`. Cost: ~10 lines across **two upstream
Rust files**. `UPSTREAM-SYNC.md` requires a failed or insufficient non-Rust
spike **plus explicit approval** before taking it. Only P01/Q7 can authorise
this, and the founder must approve.

## Files

| Path | Owner | Change |
| ---- | ----- | ------ |
| `desktop/src/features/agents/lib/runtimeCapabilities.ts` | **new, Crew-only** | descriptor type + `deriveRuntimeCapabilities(entry)` + the persona-filename table |
| `desktop/src/shared/api/acpRuntimeCatalogTypes.ts` (67 lines) | Crew-only | add `capabilities` to the TS catalog type |
| `desktop/src/shared/api/fromRawAcpRuntimeCatalog.ts` (77 lines) | Crew-only | one line: `capabilities: deriveRuntimeCapabilities(entry)` |
| `desktop/src/features/agents/lib/hermesProfileBinding.ts` | Crew-only | re-express `runtimeOwnsModelViaProfile` in terms of the descriptor; keep the exported name so callers do not churn |
| `desktop/src/features/agents/lib/agentConfigCore.ts` (upstream, +90 Crew lines already) | upstream | replace the `ownedByProfile` model omission at `:217` with an editable-write-through field for `modelSource === "profileWriteThrough"`; **~12 lines**, no restructuring |

**Upstream edits this phase:** `agentConfigCore.ts` only. Justified: the field
model is the single place that decides which controls exist, it already carries
an accepted Crew delta, and the alternative (a parallel Crew field model) is
exactly the "copied upstream implementation" anti-pattern in
`UPSTREAM-SYNC.md` § Fork-drift review.

## Turns green

E-07, E-08, E-09.

## Watch out

- `agentConfigCore.ts:317` also reads `omission.kind === "model" && omission.reason === "ownedByProfile"`.
  Both sites must move together or the model control will render twice.
- The `ownedByProfile` omission concept must survive for any future runtime that
  genuinely cannot expose a model — do not delete the reason, stop *using* it
  for Hermes.
- C-15: a non-Hermes runtime must render byte-identically to today.

## Verification

```bash
cd desktop && pnpm test -- runtimeCapabilities agentConfigCore
just desktop-typecheck
cd desktop && pnpm check && pnpm check:file-sizes
```
