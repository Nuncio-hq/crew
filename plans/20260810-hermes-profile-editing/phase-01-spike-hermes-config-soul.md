---
phase: 01
title: Spike — Hermes config + SOUL.md read/write feasibility
status: planned
priority: critical
effort: S
dependencies: []
---

# Phase 01 — Spike: Hermes config + SOUL.md read/write feasibility

Issue #118 plan item 1. This is a **read-mostly investigation**, not
implementation. It exists because the whole plan rests on four facts nobody has
verified end to end.

## Gate

D-008 / Crew workflow: spike before RED tests, RED tests before implementation.
P02–P10 must not start until this phase's evidence note exists.

## Questions to answer

| # | Question | Why it blocks | Method |
| - | -------- | ------------- | ------ |
| Q1 | Is there a non-interactive read path for the profile's model? (`hermes -p <name> config get model.default` / `model.provider` or equivalent) | P04 has nothing to read | Run against a real local profile; capture exit code + stdout shape |
| Q2 | Does `hermes -p <name> config set model.provider …` / `model.default …` succeed non-interactively, and what does it do on a **bad** model id or unauthenticated provider? | P04 error classification + R-4 (do not brick the agent) | Set a valid value, then a deliberately invalid one; record exit code and stderr first line |
| Q3 | Where does a fresh profile's default `SOUL.md` come from — a template in the Hermes install, generated text, or neither? | "Reset to Hermes default" (unresolved question 1) has no source otherwise | Inspect the Hermes install; create a throwaway profile in a temp `HERMES_HOME` and diff its `SOUL.md` |
| Q4 | Does an edited `SOUL.md` take effect on the next fresh ACP session without a respawn (same semantics as C-07 for model)? | P07 UI copy is wrong if not | Edit, `!rotate`, run a turn, observe |
| Q5 | Which `SystemPromptTransport` variant does the Hermes adapter path take, and does `prompt: None` really yield a `session/new` payload with **no** system-prompt field? | P08 contract test asserts a payload shape | Read `crates/buzz-acp/src/pool.rs:255` + `acp.rs:2355`; confirm with a captured payload |
| Q6 | Are the profile's `provider_models_cache.json` / `models_dev_cache.json` reliable enough to seed a model list? | Unresolved question 2 (free-text vs list) | Inspect structure and staleness on ≥2 real profiles |
| Q7 | Can the capability descriptor be derived from already-projected catalog facts (`profileArg`, `providerLocked`, `modelEnvVar`) for hermes / claude-code / codex, without an upstream Rust edit? | Decides Option A vs Option B | Read the catalog projection; enumerate every runtime's fact triple |

## Files to read (no writes)

| Path | For |
| ---- | --- |
| `crates/buzz-acp/src/acp.rs:2355` | `SystemPromptTransport` (S-1) |
| `crates/buzz-acp/src/pool.rs:255` | `session_new_system_prompt` (S-2) |
| `crates/buzz-acp/src/lib.rs:1942` | Layer-2 `base_prompt.md` (S-3) |
| `desktop/src-tauri/src/managed_agents/runtime.rs:680` | `BUZZ_ACP_SYSTEM_PROMPT` set/remove (S-4) |
| `desktop/src-tauri/src/managed_agents/types.rs:119` | empty description → `None` (S-5) |
| `desktop/src-tauri/src/managed_agents/hermes_profile_lifecycle.rs:153,268,362` | CLI invocation + `run_hermes` + result enum (S-9) |
| `desktop/src-tauri/src/managed_agents/discovery/runtime_metadata.rs:41,43,70` | catalog fact triple (S-7) |
| `desktop/src/shared/api/fromRawAcpRuntimeCatalog.ts:71` | projection boundary (S-8) |

## Safety rules for this spike

- Use a **throwaway profile under a temporary `HERMES_HOME`** for any `config set`
  experiment. Do not mutate `builder`, `scout`, `crewmission`, or
  `missionacceptance` beyond a value you restore immediately.
- **Never** print `auth.json`, credential values, or non-model config values.
  Key *names* only, as during planning.
- Never bind or touch the manager's default profile (`~/.hermes`) — D-019 item 1.
- Read-only on the repo. No production code in this phase.

## Deliverable

A spike note (repo convention: `docs/crew/spikes/` numbered note, or
`plans/reports/`) containing:

1. Verbatim command + exit code + redacted output for Q1, Q2, Q3.
2. A decision line for each of Q1–Q7: **confirmed / refuted / blocked**.
3. Option A vs Option B recommendation for the capability descriptor, with the
   fact triple table for every runtime in the catalog.
4. Any Hermes-side ask that must be filed (e.g. no machine-readable config read).

## Exit criteria

- Q1, Q2, Q5, Q7 answered — these gate P02/P03/P04/P08.
- Q3, Q4, Q6 answered or explicitly marked blocked with the consequence named
  (reset button dropped / UI copy adjusted / free-text model input).
- **If Q2 is refuted** (no non-interactive write with distinguishable failure):
  P04 and P06 are dropped from scope, the read-only model row stays, and the
  plan's abort criterion for P04 applies. Do not hand-edit `config.yaml` as a
  substitute.
