---
phase: 04
title: Profile model read/write IPC
status: planned
priority: high
effort: M
dependencies: [01, 02]
---

# Phase 04 — Profile model read/write IPC

Issue #118 thing-to-solve 1, backend half. Crew reads and writes the bound
profile's model **through Hermes' own CLI** and stores nothing.

## Invariant (red-team R-3)

> Crew persists no model for a profile-bound agent. The profile is the store.
> Every displayed value is a read; every write is followed by a read-back.

A React Query cache is a cache, not a source of truth — it must be invalidated
on write and on dialog open.

## Deliverable

| Command | Shape |
| ------- | ----- |
| `read_hermes_profile_model(name)` | `{ provider, model }` or a named failure |
| `write_hermes_profile_model(name, provider, model)` | result enum; on success returns the **re-read** values |

Exact CLI syntax comes from P01/Q1+Q2. Baseline documented in
`docs/crew/HERMES.md:71-72`:

```bash
hermes -p <name> config set model.provider <provider>
hermes -p <name> config set model.default <model-id>
```

## Design — reuse the shipped write-through precedent

`hermes_profile_lifecycle.rs` (S-9) is the pattern to copy, not re-invent:

| Element | `path:line` | Reuse |
| ------- | ----------- | ----- |
| `run_hermes(binary, args)` | `hermes_profile_lifecycle.rs:362` | same invocation helper |
| `first_error_line(combined)` | `hermes_profile_lifecycle.rs:378` | same error extraction |
| `create_profile_with(name, cmd)` injection seam | `hermes_profile_lifecycle.rs:153` | same testability seam — tests never shell out to a real `hermes` |
| `HermesProfileLifecycleResult` tagged enum | `hermes_profile_lifecycle.rs` | same result shape family (`Ok` / `DoesNotExist` / `BinaryMissing` / `Failed`) |
| `validate_hermes_profile_name` | `managed_agents/hermes_profile.rs` | reject `default` and malformed names before shelling out |

## Failure classification (red-team R-4 — do not brick the agent)

| Class | Cause | Surfaced as |
| ----- | ----- | ----------- |
| `BinaryMissing` | `hermes` not on the app's PATH | same copy as the existing MissingBinary path (`HERMES.md` § Failure classes) |
| `DoesNotExist` | orphaned binding | route to the existing recreate/rebind repair, do not create a profile |
| `Rejected` | invalid model id / unknown provider | first stderr line, previous value preserved |
| `Failed` | anything else | first stderr line, previous value preserved |

On any non-`Ok`, the previously read value stays displayed and remains the
profile's value — a failed write must never leave the agent with an empty model
(`model: String should have at least 1 character`, `HERMES.md:166`).

## Files

| Path | Owner | Change |
| ---- | ----- | ------ |
| `desktop/src-tauri/src/managed_agents/hermes_profile_config.rs` | **new, Crew-only** | read/write + result enum + `#[cfg(test)]` unit tests |
| `desktop/src-tauri/src/commands/hermes_profiles.rs` (Crew-only, S-10) | Crew-only | two new `#[tauri::command]` wrappers beside `list/create/delete` at `:11,17,29` |
| `desktop/src-tauri/src/lib.rs` | **upstream** | **+2 lines** in `invoke_handler`, next to `:795-797`. Justified: Tauri has no out-of-tree command registration; identical to the already-accepted Hermes lifecycle delta |
| `desktop/src/shared/api/hermesProfiles.ts` (Crew-only, S-14) | Crew-only | TS wrappers + an auditable command-line helper matching `hermesProfileCreateCommandLine` at `:62` |
| `desktop/tests/helpers/bridge.ts` | Crew-only test helper | seed model values + a failure mode for mock mode |

## Security

- Touch `model.provider` and `model.default` only. Never read, log, or render
  any other config key, and never `auth.json`.
- Never parse or rewrite `config.yaml` directly — the CLI is the only write path.
- Do not log full command output; use `first_error_line` as the lifecycle module
  already does.

## Turns green

E-01, E-02, E-03; keeps E-13 and E-14 green.

## Abort

If P01/Q2 is refuted (no non-interactive write with a distinguishable failure),
drop P04 and P06, keep the read-only model row, ship P05/P07/P08, and file the
Hermes-side ask. Do not substitute direct `config.yaml` editing.

## Verification

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml hermes_profile_config
just desktop-tauri-test
```

Manual (real profile, after the automated pass):

1. Read model for `scout` in Crew → matches `hermes -p scout config get …`.
2. Write a valid model → re-read shows it; profile file reflects it.
3. Write garbage → classified error, previous value intact, agent still runs.
