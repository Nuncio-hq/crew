## Issue-to-Plan Handoff — #118

**Decision:** proceed to plan. 10 phases in `plans/20260810-hermes-profile-editing/`.
Planning-only run — no branch, no PR, no code. All 5 DoD checkboxes map to ≥1 phase.

### Phases

| # | Title | Effort | Depends |
| - | ----- | ------ | ------- |
| 01 | Spike — Hermes config + SOUL.md read/write feasibility | S | — |
| 02 | RED contract tests (17 contracts) | M | 01 |
| 03 | Runtime capability descriptor | S | 01, 02 |
| 04 | Profile model read/write IPC | M | 01, 02 |
| 05 | SOUL.md read/write/reset IPC | M | 01, 02 |
| 06 | Model write-through UI | M | 03, 04 |
| 07 | SOUL.md editor UI + persona at birth | M | 03, 05 |
| 08 | Description optionality + layer copy | S | 02 |
| 09 | Docs truth + STATE anti-drift | M | 06, 07, 08 |
| 10 | Verification & evidence | S | 09 |

### Key design decisions

1. **Crew is a remote control, not a second store** — every write goes through
   Hermes' own CLI/file with a read-back; Crew persists no model and no persona.
2. **Capability descriptor with zero upstream Rust edits (Option A)** — derive
   `{modelSource, personaDoc, layer3}` at the Crew-owned catalog projection from
   `profileArg`/`providerLocked`/`modelEnvVar`. Declaring it on `KnownAcpRuntime`
   (Option B) costs 2 upstream files + `UPSTREAM-SYNC.md` approval — escalation only.
3. **D-019 item 2 superseded in its presentation half only.** `HERMES.md` rule 2,
   feature 0001 C-04, and `desktop/src/features/agents/AGENTS.md` rules 3+8 all say
   the opposite today and are updated in the same PR (new decision **D-028**).
4. **Layer-3 optionality is mostly already shipped** (`types.rs:119` maps empty →
   `None`) — Phase 08 is contract-proof + copy + docs, not a new mechanism.
5. Upstream budget: `lib.rs` +4 (command registration), agents `AGENTS.md` ~15,
   both agent dialogs ≤10 each — mount-only, since both are already over the
   1000-line ratchet, so all new markup lives in Crew-owned children.

### Named Buzz seams

`buzz-acp/src/acp.rs:2355` (`SystemPromptTransport`) · `pool.rs:255`
(`session_new_system_prompt`) · `lib.rs:1942` (L2 `base_prompt.md`) ·
`managed_agents/runtime.rs:680` (`BUZZ_ACP_SYSTEM_PROMPT`) · `types.rs:119`
(empty→`None`) · `types/crew.rs:7` · `discovery/runtime_metadata.rs:41,43,70` ·
`hermes_profile_lifecycle.rs:153,268,362` (CLI write-through precedent) ·
`commands/hermes_profiles.rs:11` · `shared/api/fromRawAcpRuntimeCatalog.ts:71` ·
`agents/lib/agentConfigCore.ts:217` · `ui/EditAgentModelAndProfileSection.tsx:71`

### Open questions (none block Phase 01)

1. **Reset-to-default source for `SOUL.md`** — Hermes template, throwaway-profile
   copy, or none. A bundled Crew copy is rejected (drift); with no trustworthy
   source the editor ships without the reset button.
2. **Model input shape** — recommend free-text + validation; `get_agent_models`
   is keyed on adapter env/provider config, not on a Hermes profile.
3. **Does a `SOUL.md` edit apply without respawn?** UI copy depends on it.
4. **Option A vs B** for the descriptor — B needs founder approval.

Validate **pass** (11 checks, 1 revision). Red-team **6 findings, 5 applied, 1
recorded-not-applied**. Details in `plan.md`.
