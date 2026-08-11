# Hermes profile editing from Crew (model write-through, SOUL.md, optional description)

- **Status:** Planned — not implemented, not approved for cook
- **Date:** 2026-08-10
- **Issue:** [#118](https://github.com/Nuncio-hq/crew/issues/118)
- **Parent feature:** [`docs/crew/features/0001-hermes-first-class-runtime.md`](../../docs/crew/features/0001-hermes-first-class-runtime.md)
- **Related plans:** [`../20260805-1330-hermes-first-class-runtime/plan.md`](../20260805-1330-hermes-first-class-runtime/plan.md),
  [`../20260806-1225-hermes-profile-picker/plan.md`](../20260806-1225-hermes-profile-picker/plan.md)
- **Decisions touched:** D-019 (item 2 partially superseded), D-020, D-023,
  D-024, D-025 — **one new decision expected (D-028)**
- **Branch (suggested):** `agents/hermes-profile-editing` (area name, not a
  phase number — `UPSTREAM-SYNC.md` branch rule)
- **Target repo:** `Nuncio-hq/crew` only. Never `block/buzz` (D-020).

> This plan is **planning-only output**. No production code, no branch push,
> no PR, no issue comment was produced by the planning run.

## Manager-visible outcome

Oscar opens a Hermes agent in Crew and can **see and change the model**, and
**read and edit the profile's `SOUL.md`** — the words that decide who the
agent is — without leaving the app for a terminal. Crew tells him plainly
that both belong to the profile and therefore change everywhere that profile
runs. A profile he creates from Crew is born with a real persona instead of
the generic default. The Crew "Agent instructions" box becomes genuinely
optional: leave it empty and nothing extra is injected.

Founder direction locked in the issue: **"Crew does not own the person, but
Crew is where you edit the person."**

## What changes about a previously recorded rule

D-019 item 2 and [`docs/crew/HERMES.md`](../../docs/crew/HERMES.md) rule 2 say
Crew shows `"Model: decided by profile <name>"` and **never** offers an
editable model control. `desktop/src/features/agents/AGENTS.md` rule 3 says the
same in implementer language. Issue #118 supersedes **only the presentation
half** of that rule.

| Still true (do not reopen) | Superseded by #118 |
| -------------------------- | ------------------ |
| The profile is the single source of truth for model/provider | Crew may only *display* the model |
| Crew stores no model for a profile-bound agent | The UI must not offer a model control |
| `BUZZ_ACP_MODEL` stays stripped at spawn for profile-locked runtimes | — |
| Model change takes effect on next fresh ACP session (C-07) | — |

Crew becomes a **remote control for the profile**, not a second store. Every
write goes through Hermes' own CLI; Crew persists nothing.

## Named Buzz seams (every feature hangs off one)

| # | Seam | `path:line` | Used by |
| - | ---- | ----------- | ------- |
| S-1 | `SystemPromptTransport` enum (Layer-3 delivery shape) | `crates/buzz-acp/src/acp.rs:2355` | P01, P08 |
| S-2 | `session_new_system_prompt()` — picks `Field` / `ClaudeMeta` / `None` | `crates/buzz-acp/src/pool.rs:255` | P01, P08 |
| S-3 | Layer-2 harness prompt source (`base_prompt.md`) | `crates/buzz-acp/src/lib.rs:1942` | P08, P09 |
| S-4 | Spawn env: sets or removes `BUZZ_ACP_SYSTEM_PROMPT` | `desktop/src-tauri/src/managed_agents/runtime.rs:680` | P08 |
| S-5 | Empty description already normalises to `None` | `desktop/src-tauri/src/managed_agents/types.rs:119` | P02, P08 |
| S-6 | Crew-owned IPC catalog entry (capability projection) | `desktop/src-tauri/src/managed_agents/types/crew.rs:7` | P03 |
| S-7 | Rust catalog facts (`profile_arg`, `provider_locked`, `model_env_var`) | `desktop/src-tauri/src/managed_agents/discovery/runtime_metadata.rs:41,43,70` | P03 |
| S-8 | Crew-owned raw→TS catalog projection | `desktop/src/shared/api/fromRawAcpRuntimeCatalog.ts:71` | P03 |
| S-9 | Hermes CLI invocation + result-enum precedent | `desktop/src-tauri/src/managed_agents/hermes_profile_lifecycle.rs:153,268,362` | P04, P05 |
| S-10 | Crew-owned Hermes IPC commands module | `desktop/src-tauri/src/commands/hermes_profiles.rs:11` | P04, P05 |
| S-11 | Field model: `ownedByProfile` model omission | `desktop/src/features/agents/lib/agentConfigCore.ts:217` | P03, P06 |
| S-12 | Model row vs editable picker branch | `desktop/src/features/agents/ui/EditAgentModelAndProfileSection.tsx:71` | P06 |
| S-13 | `ProfileOwnedModelRow` (copy #118 replaces) | `desktop/src/features/agents/ui/HermesProfileBindingFields.tsx:40` | P06 |
| S-14 | Crew-owned TS IPC wrappers for Hermes profiles | `desktop/src/shared/api/hermesProfiles.ts:41` | P04, P05 |
| S-15 | "Agent instructions" textarea (Layer-3 author surface) | `desktop/src/features/agents/ui/AgentDefinitionDialog.tsx:837` | P07, P08 |

## Architecture decision — capability descriptor without an upstream Rust edit

The issue requires a per-runtime capability descriptor so render code never
branches on `runtime.id === "hermes"` (D-025; `desktop/src/features/agents/AGENTS.md`
rule 1). Two ways to get one:

| Option | Shape | Upstream cost |
| ------ | ----- | ------------- |
| **A (chosen)** | Derive `{ modelSource, personaDoc, layer3 }` in a **Crew-only** module from catalog facts already projected (`profileArg`, `providerLocked`, `modelEnvVar`), computed once at the `fromRawAcpRuntimeCatalog` boundary (S-8) | **zero** |
| B (escalation only) | Declare the descriptor on `KnownAcpRuntime` (S-7) and project it through `discovery.rs:1273` | 2 upstream Rust files, ~10 lines, requires the failed-non-Rust-spike + approval rule in `UPSTREAM-SYNC.md` |

**Option A is the default.** `UPSTREAM-SYNC.md` states a Rust edit to an
upstream file "requires a failed or insufficient non-Rust spike plus explicit
approval" — so B may only be taken if P01 proves A insufficient, and then only
with founder approval recorded in the plan and in `DECISIONS.md`.

Under A the capability facts still come from the Rust catalog; Crew only
*projects* them into a named descriptor at one boundary. Render code reads the
descriptor. The one fact A cannot read from the catalog is the persona
filename (`SOUL.md`); it is named once, as data, in the Crew-only capability
table — never as a branch inside a component.

Descriptors (issue §Design constraint):

```text
Hermes                : { modelSource: "profileWriteThrough", personaDoc: "soulMd", layer3: "append" }
Claude Code / Codex   : { modelSource: "adapterSetting",      personaDoc: "none",   layer3: "append" }
```

## Honest scoping note (do not inflate this work)

**Layer-3 optionality is already mostly true in code.** `types.rs:119` (S-5)
already maps an empty description to `None`, and `AgentDefinitionDialog`'s
submit gate does not require a description. Phase 08 is therefore
**contract-proof + copy + docs**, not a new mechanism. The plan says so rather
than inventing work to fill a DoD box. What is genuinely unproven is *which*
`SystemPromptTransport` variant (S-1/S-2) the Hermes adapter path takes and
whether `None` really produces a payload with no system-prompt field — that is
a P01 spike item and a P02 RED test.

## Scope

| # | Phase | Ships | Depends |
| - | ----- | ----- | ------- |
| 01 | [Spike — Hermes config + SOUL read/write feasibility](phase-01-spike-hermes-config-soul.md) | Evidence note; go/no-go per mechanism | — |
| 02 | [RED contract tests](phase-02-red-contract-tests.md) | Failing Rust + TS + Playwright contracts | 01 |
| 03 | [Runtime capability descriptor](phase-03-runtime-capability-descriptor.md) | `{modelSource, personaDoc, layer3}` at the catalog boundary | 01, 02 |
| 04 | [Profile model read/write IPC](phase-04-profile-model-ipc.md) | `read/write_hermes_profile_model` over the Hermes CLI | 01, 02 |
| 05 | [SOUL.md read/write/reset IPC](phase-05-profile-soul-ipc.md) | `read/write/reset_hermes_profile_soul` | 01, 02 |
| 06 | [Model write-through UI](phase-06-model-write-through-ui.md) | Editable model + shared-everywhere note (edit + create) | 03, 04 |
| 07 | [SOUL.md editor UI + persona at birth](phase-07-soul-editor-ui.md) | Edit-in-place editor; persona step on create-in-place | 03, 05 |
| 08 | [Description optionality + layer copy](phase-08-description-optionality.md) | Empty = no Layer-3; layer semantics in the UI | 02 |
| 09 | [Docs truth + STATE anti-drift](phase-09-docs-and-state.md) | D-028, HERMES.md, feature 0001, STATE.md, agents AGENTS.md | 06, 07, 08 |
| 10 | [Verification & evidence](phase-10-verification-evidence.md) | Live probe, Playwright shots, `just ci` | 09 |

## Definition of Done → phase map (issue #118)

| DoD checkbox (issue) | Phases |
| -------------------- | ------ |
| 1. Model visible **and** editable from Crew for a Hermes agent, with the shared-everywhere note; new-profile flow can set a model | 01, 02, 03, 04, 06 |
| 2. `SOUL.md` readable/editable from Crew; Crew-created profiles get a real persona at birth (not the generic default) | 01, 02, 05, 07 |
| 3. Crew description optional; empty = no Layer-3 injection; layer semantics documented | 02, 08, 09 |
| 4. Decision recorded; `HERMES.md` hiring flow updated; feature 0001 Slice 2 (C-03…C-12) reconciled; `STATE.md` updated in the same PR | 09 |
| 5. Contract tests + live probe + Playwright evidence attached to the PR | 02, 10 |

No DoD checkbox is unmapped. No phase exists without a DoD or issue-plan line
behind it.

## Non-goals (from the issue — do not drift)

- No Crew editor for profile **memory**, **skills**, or **credentials**.
- No change to the `BUZZ_ACP_MODEL` strip-at-spawn guard (C-05/C-06 stay green).
- No new engine implementations. Hermes is the only adapter; the UI seam must
  still be capability-shaped so a future engine can declare its own descriptor.
- Not a rewrite of shipped binding / readiness / lifecycle UI.
- No auth badge (still blocked on the Hermes-side probe, spike 0010).
- Crew never stores model, provider, or persona text for a profile-bound agent.

## Thin-fork budget (upstream files)

| File | Owner | Expected delta | Justification |
| ---- | ----- | -------------- | ------------- |
| `desktop/src-tauri/src/lib.rs` | upstream | **+4 lines** — register new Hermes commands in `invoke_handler` next to `lib.rs:795-797` | Tauri has no out-of-tree command registration; identical to the already-accepted `list/create/delete_hermes_profile` delta |
| `desktop/src/features/agents/AGENTS.md` | upstream | **~15 lines** — rule 3 and rule 8 currently forbid an editable model control | Its own closing rule requires updating it in the same PR that changes how config is modelled/rendered; leaving it stale would make the file lie |
| `desktop/src/features/agents/ui/AgentDefinitionDialog.tsx` | upstream | **≤10 lines** — mount Crew-owned child components + optional-label copy | Already **1016 lines**, over the 1000-line ratchet: all new markup goes into Crew-owned children (D-022 direction), never into this file |
| `desktop/src/features/agents/ui/AgentInstanceEditDialog.tsx` | upstream | **≤10 lines** — same | Already **1224 lines**; same rule |
| `desktop/src-tauri/src/managed_agents/discovery/runtime_metadata.rs` | upstream | **0 (target)** / ~8 if Option B is approved | Only if P01 proves the Crew-side derivation insufficient |
| `desktop/src-tauri/src/managed_agents/discovery.rs` | upstream | **0 (target)** / ~2 if Option B is approved | Same |

Everything else is **additive Crew-owned files**: `commands/hermes_profiles.rs`,
`managed_agents/hermes_profile_config.rs` (new), `managed_agents/hermes_profile_soul.rs`
(new), `shared/api/hermesProfiles.ts`, `features/agents/lib/runtimeCapabilities.ts`
(new), `features/agents/ui/HermesProfileModelField.tsx` (new),
`features/agents/ui/HermesSoulEditor.tsx` (new).

**File-size guard:** `desktop/scripts/check-file-sizes.mjs` enforces
`MAX_LINES = 1000`. Both agent dialogs already exceed it. Never raise the limit
and never add an override — extract into Crew-owned children (D-022).

## Generic-ACP check (D-025)

| Mechanism | Generic across engines? | Hermes-only part |
| --------- | ----------------------- | ---------------- |
| Capability descriptor on the runtime catalog | **Yes** — every runtime gets one | Hermes' values |
| Layer-3 description → `SystemPromptTransport` (S-1/S-2) | **Yes** — unchanged Buzz contract for all engines | none |
| Empty description → no injection (S-5) | **Yes** | none |
| Model write-through via profile CLI | **No** | Labelled `modelSource: profileWriteThrough`; other engines keep `adapterSetting` |
| `SOUL.md` persona editor | **No** | Labelled `personaDoc: soulMd`; others render nothing |

Per `FOUNDER-PRODUCT.md` rule 4, UI copy and docs must say which parts are
Hermes-only. Non-Hermes runtimes must render **exactly** what they render today
(C-15 stays green).

## Security and privacy notes

- **Never read or render `auth.json`.** Model write-through touches
  `config.yaml` keys `model.provider` / `model.default` only, and only through
  `hermes -p <name> config set …` — Crew must not parse or rewrite the profile
  YAML directly.
- **Never log or echo profile config values** that are not the model id/provider.
  During planning, only key *names* were inspected on the live machine; the same
  discipline applies to implementation and to any PR evidence.
- `SOUL.md` is founder-authored prose and may contain sensitive business
  context. It stays local; it is never sent to a relay, never put in an issue,
  never in a screenshot posted to a PR unless the founder approves that specific
  text.
- D-024 still holds: profile-bound Hermes agents are owner-only and local.
  Nothing in this plan opens a remote or allowlisted path to profile editing.
- The credential-fallback caveat (`HERMES.md` § Security caveats, spike 0010) is
  unchanged: a fresh profile can spend the manager's provider credit. Changing a
  model from Crew must not imply Crew is provisioning credentials.

## Backward compatibility / migration

None required. No stored schema changes: Crew persists no model and no persona
for profile-bound agents. Existing bindings, records, and legacy tier-3 custom
harness JSONs behave exactly as today. Agents whose Crew description is
non-empty keep injecting Layer 3 exactly as today.

## Rollback / abort criteria

- Revert the PR. The UI returns to `ProfileOwnedModelRow` (S-13), the new IPC
  commands disappear, and no profile is left in a half-written state — every
  write is a single idempotent Hermes CLI call or a single file write.
- **Abort P04 (model write-through)** if P01 finds no non-interactive
  `hermes -p <name> config set` path with a distinguishable failure exit — fall
  back to shipping P05/P07/P08 and keeping the read-only model row, and record
  the Hermes-side ask. Do not scrape or hand-edit `config.yaml` as a substitute.
- **Abort the "Reset to Hermes default" button only** (not the whole editor) if
  P01 finds no trustworthy source for the default `SOUL.md`. Editing in place
  still ships; the reset affordance is dropped and recorded as a known gap.
- If any of C-05, C-06, C-10, C-13, C-14, C-15 regress, stop and revert — those
  are shipped guarantees, not negotiable.

## Testing and validation plan

| Level | What | Where |
| ----- | ---- | ----- |
| Rust unit | model read/write result enum, error classification, name validation reuse | `hermes_profile_config.rs`, `hermes_profile_soul.rs` (colocated `#[cfg(test)]`) |
| Rust unit | SOUL round-trip, missing-profile, unreadable-file, reset source | same |
| Rust contract | empty description → `session/new` payload with no system-prompt field (S-1/S-2/S-5) | `crates/buzz-acp` test module |
| TS unit | capability descriptor derivation for hermes / claude-code / codex / unknown runtime | `runtimeCapabilities.test` |
| TS unit | field-model change: `ownedByProfile` → editable-with-write-through (S-11) | `agentConfigCore` tests |
| Playwright (mock bridge) | three UI states from the issue: profile model editable + note, SOUL editor with real content, empty description accepted | `desktop/tests/e2e/hermes-profile-binding.spec.ts` (+ new spec if it outgrows the file) |
| Live probe | real profile on this machine: change model from Crew, verify with `hermes -p <name> config get`, run a turn | P10 |
| Gate | `just ci`; `just desktop-tauri-test`; `pnpm check:file-sizes`; `pnpm check:px-text` | P10 |

**Build rule for e2e:** always `pnpm test:e2e:smoke` / `pnpm build:e2e` — a plain
`pnpm run build` strips the mock bridge and every mock-mode spec fails in a way
that looks like a product bug.

## Unresolved questions (for the founder — none block starting P01)

1. **Reset-to-default source of truth for `SOUL.md`.** Three candidates: create
   a throwaway profile and copy its `SOUL.md` (side-effecty), bundle a copy in
   Crew (drifts from Hermes), or read a template from the Hermes install (best
   if one exists). P01 must answer; if none is trustworthy, ship the editor
   without the reset button.
2. **Model input shape.** Free-text model id with validation, or a list?
   Recommendation: **free-text + validation**, optionally seeded from the
   profile's own `provider_models_cache.json` / `models_dev_cache.json` if P01
   shows they are reliable. `get_agent_models` (`commands/agent_models.rs:36`)
   is keyed on adapter env/provider config, not on a Hermes profile, so reusing
   it would be a false promise.
3. **Does a `SOUL.md` edit apply without respawn?** C-07 says a model change is
   picked up on the next fresh ACP session (`!rotate` forces one). P01 must
   confirm the same is true for `SOUL.md`, because the UI copy depends on the
   answer.
4. **Descriptor placement.** Option A (Crew-only derivation, zero upstream
   edits) is the plan's default. Approving Option B is a founder call and needs
   the `UPSTREAM-SYNC.md` Rust-edit approval.

## Validation pass

Run against `/ak:plan validate` criteria on 2026-08-10. **Result: pass.**

| Check | Outcome |
| ----- | ------- |
| Objective, scope, non-goals present | Pass — non-goals copied from the issue verbatim in intent |
| Every DoD checkbox mapped to ≥1 phase | Pass — 5/5 mapped, table above |
| Phases ordered by real dependency | Pass — Spike → RED → mechanism → UI → docs → evidence |
| Every feature names a Buzz seam with `path:line` | Pass — 15 seams, all line numbers verified against the working tree |
| Upstream-file edits justified with expected diff size | Pass — 6 rows, 4 targeted at zero |
| Workflow gate order (D-008: spike → RED → implement) | Pass — P01, P02 precede all implementation phases |
| Testing/validation plan present | Pass |
| Security/privacy notes present | Pass — credential and `SOUL.md` handling called out |
| Rollback/abort criteria present | Pass — per-phase aborts, not just "revert" |
| Migration/back-compat addressed | Pass — none required, stated with reason |
| No implementation performed during planning | Pass — files on disk are plan documents only |

**Revision made during validation:** the first draft mapped DoD-3 to a new
mechanism phase. Verifying `types.rs:119` showed empty descriptions already
normalise to `None`, so P08 was rewritten as contract-proof + copy + docs. The
plan now states this explicitly rather than claiming credit for shipped
behaviour.

## Red-team pass

Six findings, **five applied**, one recorded-not-applied.

| # | Finding | Disposition |
| - | ------- | ----------- |
| R-1 | The plan silently reverses D-019 item 2, `HERMES.md` rule 2, and `AGENTS.md` rule 3 — three places tell an implementer the opposite | **Applied.** Added the "What changes about a previously recorded rule" table showing what is superseded vs still true, and made P09 update all three in the same PR. Nothing is reversed silently. |
| R-2 | "Capability descriptor" is a euphemism for adding a Hermes branch one layer down; `SOUL.md` cannot be derived from `profileArg` alone | **Applied.** Named the limit honestly: the persona filename is data in one Crew-owned table, never a render branch, and Option B is the recorded escalation if that is not good enough. |
| R-3 | Write-through invites Crew becoming a second store — a cached model value would silently diverge from the profile | **Applied.** Added the invariant "Crew persists nothing" to the outcome, the D-019 table, back-compat, and P04/P06 (read-after-write from the profile, no local cache as source of truth). |
| R-4 | The model editor could brick a working agent (bad model id ⇒ `model: String should have at least 1 character` class failure) with no undo | **Applied.** P04 must classify write failures and P06 must show the previous value and keep it recoverable; the abort criterion for P04 is explicit. |
| R-5 | "Reset to Hermes default" has no defined source; a bundled copy would drift and could overwrite founder-authored prose | **Applied.** Promoted to unresolved question 1 with a per-affordance abort criterion; the editor ships without reset rather than guessing. Never a blank-replace box (issue rule). |
| R-6 | Ten phases is heavy for one issue; P03/P04/P05 could be one Rust phase | **Not applied.** Recorded rationale: they carry different risk (pure projection vs CLI write-through vs file IO with an unresolved default source) and different abort criteria. Merging them would hide R-4 and R-5 behind one green checkbox. Splitting keeps each abort independently exercisable. |

**Residual risk accepted:** if the founder edits `SOUL.md` in Crew while a
Hermes session is live, the change lands on the next fresh session, not the
current turn (same semantics as C-07 for model). P07 must say so in the UI. This
is a property of the profile lifecycle, not a defect introduced here.

## Approval checkpoint

No production implementation until this plan is approved. On approval the order
is fixed: **P01 spike → P02 RED → P03…P08 → P09 docs → P10 evidence**, one PR
series into `Nuncio-hq/crew` `main` through `NuncioCrew Gate`, commits signed
off (`git commit -s`, DCO).
