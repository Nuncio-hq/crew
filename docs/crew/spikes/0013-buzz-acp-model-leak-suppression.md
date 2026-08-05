# Spike 0013 — `BUZZ_ACP_MODEL` leak paths and suppression point for Hermes

- **Status:** PASS (analysis; enforcement point identified)
- **Date:** 2026-08-05
- **Feature:** [`../features/0001-hermes-first-class-runtime.md`](../features/0001-hermes-first-class-runtime.md) (S0-5)

## Question

Through which code paths can a model value reach a Hermes agent's spawn
environment, and where can Crew suppress it (P-3) with the smallest,
most additive change that satisfies both C-05 (field resolution) and
C-06 (env-var maps)?

## Decision affected

P-3 (no model injection for Hermes), the Slice 2 implementation plan,
and the upstream tier-1 entry shape (whether upstream alone already
solves part of the leak).

## Hypothesis

Two independent leak paths exist (field resolution → `BUZZ_ACP_MODEL`;
user env maps), so a single-field fix is insufficient.

## Scope

Code reading only (no production edits): `desktop/src-tauri/src/managed_agents/`
spawn path and `crates/buzz-acp` model plumbing, at commit `40773ea6d`.

## Exclusions

- No implementation. No decision on exact new-file layout for Slice 2.

## Pass criteria

Every write of a model value into the spawn env is enumerated with
`path:line`, and at least one enforcement point covering all of them is
identified with an expected diff shape.

## Fail criteria

A leak path that cannot be intercepted without a large upstream rewrite.

## Results — leak inventory

Model values can reach a Hermes agent process via **three** writes (one
more than hypothesized):

1. **Effective-config field resolution → `BUZZ_ACP_MODEL`**
   (`runtime.rs:766-770`). Source chain: linked persona definition →
   global default (`global_config/mod.rs:42-45`,
   `resolve_effective_prompt_model_provider` in
   `runtime/metadata.rs:65-84`); definition-less instances: record →
   global.
2. **Runtime-metadata env vars** (`runtime.rs:781-791` →
   `runtime_metadata_env_vars`, `runtime/metadata.rs:8-25`): writes
   `meta.model_env_var`/`provider_env_var` for known runtimes. For
   Hermes today `runtime_meta` is `None` (tier-2), so this path is
   currently inert — but it becomes live exactly when the upstream
   tier-1 entry lands, and is then **controlled by the upstream entry
   itself**: `model_env_var: None, provider_env_var: None,
   provider_locked: true` (the Claude shape, `discovery.rs:124-126`)
   permanently disables it.
3. **User env maps → `descriptor.env`** (`runtime.rs:859-861`): the
   fully-layered user env (definition floor → global `env_vars` →
   persona → agent) is written **last**, so a `BUZZ_ACP_MODEL` entry in
   any of those maps wins over everything. `BUZZ_ACP_MODEL` is
   deliberately not a reserved key
   (`env_vars/tests.rs:135` — "behavior knob"), so `merged_user_env`
   passes it through.

Downstream consequence of a leak: buzz-acp stores the value as
`desired_model` and re-applies it after **every** `session/new`
(`pool.rs:179-184`, `apply_model_switch` `pool.rs:1036`), silently
overriding the profile's configured model when the id happens to exist
in the profile's catalog (worst case of C-05/C-06). Failures are
non-fatal (`pool.rs:1032-1034`), which softens but does not fix the
silent-override case.

## Results — enforcement point

The three writes share one funnel: the spawn-command assembly in
`spawn_agent_command` (`runtime.rs`), and writes 1 and 3 both live
there. The smallest complete enforcement is a **single guard applied
after the last env write, before spawn**:

- For records whose resolved runtime identity is Hermes
  (`known_acp_runtime`-style normalization of the effective command —
  same normalization upstream `buzz-acp` uses,
  `crates/buzz-acp/src/config.rs:714`):
  `command.env_remove("BUZZ_ACP_MODEL")` (and skip/strip provider
  analogously if a provider var ever applies).
- Implemented as an additive helper in a new Crew-owned module (e.g.
  `managed_agents/hermes_profile.rs` or similar), called from one line
  in `runtime.rs` after the `descriptor.env` loop (`runtime.rs:861`) —
  inside the existing one-edit budget style. `spawn_config_hash` must
  hash the same post-guard view so the restart badge cannot disagree
  (same invariant the file already documents at `runtime.rs:745-749`).

Expected diff: 1 new file (~40 lines + tests) + 1–2 call-site lines in
`runtime.rs`. The persona-editor field suppression (C-04) is a separate
TS-side change and does not affect this guard's completeness because
the guard is last-write.

Division of labor confirmed: upstream tier-1 entry closes path 2 by
declaration; the Crew guard closes paths 1 and 3; C-05/C-06 tests pin
both.

## Edge cases observed

- The runtime model picker (live ACP switch, `SwitchModel` control) is
  intentionally out of scope of the guard — it is session-scoped and
  resets on respawn (`pool.rs:181-184`), matching P-2.
- The guard must run on the *effective* command (after
  `agent_command_override` and persona runtime resolution,
  `record_agent_command`, `discovery.rs:299`), not on the record's raw
  runtime string.

## Limitations

- Static analysis only; the guard's interaction with `spawn_config_hash`
  is asserted from code comments, not an executed test — the Slice 2 RED
  tests cover it.

## Verdict

**PASS** — three leak paths enumerated; a single last-write guard plus
the upstream entry's declarative `None`s covers all of them within
thin-fork budgets.

## Follow-up test contract (RED before Slice 2)

- C-05: global default model set → Hermes spawn env has no
  `BUZZ_ACP_MODEL`; goose spawn unaffected (C-15).
- C-06: `BUZZ_ACP_MODEL` in global/persona/agent env maps → stripped for
  Hermes runtime, preserved for others.
- Restart-badge invariant: guard applied ⇒ `spawn_config_hash` equality
  unaffected by suppressed values.

## Cleanup

No code changed; no artifacts.
