---
phase: 02
title: RED contract tests
status: planned
priority: critical
effort: M
dependencies: [01]
---

# Phase 02 — RED contract tests

Issue #118 plan item 6 ("Contract tests first (RED)"). Every contract below
must **fail** before any implementation phase starts, and each names the phase
that turns it green.

## Contracts

| ID | Scenario | Expected | Forbidden | Turns green in |
| -- | -------- | -------- | --------- | -------------- |
| E-01 | Read model for a bound profile | Crew reports the profile's current `model.provider` + `model.default` | Crew reading or writing `config.yaml` directly | 04 |
| E-02 | Write a valid model from Crew | Profile reflects the new value on re-read; Crew stores nothing locally | A Crew-side cached model treated as source of truth | 04 |
| E-03 | Write an invalid model id / unauthenticated provider | Classified failure surfaced with the previous value still intact and recoverable | Silent success, or an agent left unable to run a turn | 04 |
| E-04 | Read `SOUL.md` for an existing profile | Real current file content returned | Empty string standing in for "couldn't read" | 05 |
| E-05 | Write `SOUL.md` round-trip | Byte-exact round-trip; unchanged file when the editor is opened and closed without edits | Truncation, re-encoding, or blank-replace | 05 |
| E-06 | `SOUL.md` for a missing / orphaned profile | Named failure (reuse the `hermes_profile_lifecycle` result-enum shape, S-9) | Panic, `unwrap`, or a silently created profile directory | 05 |
| E-07 | Capability descriptor for hermes | `{ modelSource: "profileWriteThrough", personaDoc: "soulMd", layer3: "append" }` | Any `runtime.id === "hermes"` comparison in render code | 03 |
| E-08 | Capability descriptor for claude-code / codex / unknown runtime | `{ modelSource: "adapterSetting", personaDoc: "none", layer3: "append" }` | Hermes-only UI leaking into another runtime (C-15) | 03 |
| E-09 | Field model for a profile-bound runtime | Model field is **editable with write-through**, no longer an `ownedByProfile` omission (S-11, `agentConfigCore.ts:217`) | Silent removal of the omission concept for non-profile runtimes | 03, 06 |
| E-10 | Empty Crew description at spawn | `BUZZ_ACP_SYSTEM_PROMPT` is **removed**, not set empty (S-4, `runtime.rs:680`) | An empty-string env var | 08 |
| E-11 | Empty Crew description at `session/new` | Payload carries **no** system-prompt field at all (S-1/S-2/S-5) | `systemPrompt: ""` or `_meta.systemPrompt: {append: ""}` | 08 |
| E-12 | Non-empty description | Existing Layer-3 append behaviour unchanged for every engine | Any change to the shipped injection path | 08 |
| E-13 | `BUZZ_ACP_MODEL` strip guard for profile-locked runtimes | Still stripped at spawn (C-05/C-06) | Re-introducing the env var because Crew now edits models | 04, 06 |
| E-14 | Duplicate binding, keep/delete offboarding, orphan repair | C-10, C-13, C-14 unchanged | Regression from new IPC surface | 04, 05 |
| E-15 | Playwright: Hermes agent open in edit | Editable model control + the shared-everywhere note visible | The old read-only `profile-owned-model-row` copy | 06 |
| E-16 | Playwright: SOUL editor | Opens populated with real current content; save persists | A blank textarea presented as the persona | 07 |
| E-17 | Playwright: empty description | Create/save succeeds with an empty "Agent instructions" box, labelled optional | A required-field block | 08 |

## Where the tests live

| Layer | Location |
| ----- | -------- |
| Rust unit (model + soul) | colocated `#[cfg(test)]` in the new Crew-only `hermes_profile_config.rs` / `hermes_profile_soul.rs` |
| Rust contract (transport payload) | `crates/buzz-acp` test module next to the existing `base_prompt` tests (`lib.rs:4547` area) |
| Rust contract (spawn env) | `desktop/src-tauri/src/managed_agents` tests near the existing runtime tests |
| TS unit | `desktop/src/features/agents/lib/runtimeCapabilities.test.*`, `agentConfigCore` tests |
| Playwright | `desktop/tests/e2e/hermes-profile-binding.spec.ts`; split into a sibling spec if the file grows past the ratchet |

## Mock bridge work

`desktop/tests/helpers/bridge.ts` already exposes `hermesProfiles?: string[]`
(the `list_hermes_profiles` mock). E-15…E-17 need the same treatment for the new
commands: seeded model values and seeded `SOUL.md` content, plus a failure mode
so E-03's UI path is exercisable in mock mode.

## Verification

```bash
# Rust (desktop crate is NOT in the root workspace)
cargo test --manifest-path desktop/src-tauri/Cargo.toml hermes_profile
cargo test -p buzz-acp system_prompt

# TS
cd desktop && pnpm test -- runtimeCapabilities agentConfigCore

# Playwright — build:e2e is mandatory, a plain build strips the mock bridge
cd desktop && pnpm test:e2e:smoke
```

## Exit criteria

All 17 contracts exist and **fail for the right reason** (missing behaviour, not
a typo or a missing mock). Record the RED run output; it is part of the DoD-5
evidence attached to the PR.
