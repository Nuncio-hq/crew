---
phase: 02
title: Readiness model, evaluator, and projection
status: planned
priority: P0
effort: M (2-3 d)
dependencies: ["01"]
---

# Phase 02 — Readiness model, evaluator, and projection

## Outcome

Crew stops asking one question about a Hermes profile. The five named
states from the issue exist in Rust, are produced by a real evaluator,
and reach the frontend through the existing named-reason pipeline —
without any frontend rival table and without a `runtime.id` check
anywhere.

DoD coverage: #1 (model + evaluator + projection half), #5
(`auth-unknown` as an honest advisory field).

## Design constraints inherited from the plan

- **DD-1:** `missing` / `broken-config` / `binary-missing` are
  **blocking** and become `Requirement` variants. `ready` /
  `auth-unknown` are **non-blocking advisory** and ride a separate
  field. Making `auth-unknown` a `Requirement` would push every healthy
  Hermes agent into setup-listener mode via `runtime.rs` — a full
  outage of the runtime.
- **DD-2:** the carrier is the per-agent `Requirement` pipeline plus the
  runtime status projection, **not** `AcpRuntimeCatalogEntry` or
  `lib/agentConfigCore.ts`. The catalog holds harness-scoped capability
  facts and `agentConfigCore.ts` projects field descriptors
  (`desktop/src/features/agents/AGENTS.md` rule 1); per-agent,
  per-machine, time-varying readiness belongs in neither.
- **D-025:** the `Requirement` boundary is generic. Everything
  Hermes-specific (YAML parse, `hermes --version`, profile dir layout)
  stays inside `readiness/hermes.rs`, explicitly labelled as
  Hermes-specific in doc comments.
- **DD-7:** RED tests first, in this PR, with observed failure output in
  the PR body; then implement to green.

## Seams

| Seam | Use |
| ---- | --- |
| `managed_agents/readiness.rs:284` (`Requirement`), `:333` (`HermesProfileDirectoryMissing`) | Add sibling variants |
| `managed_agents/readiness.rs:340` (`AgentReadiness`) | Shape unchanged |
| `managed_agents/readiness/hermes.rs` (`hermes_requirements`) | Where the new checks land |
| `managed_agents/hermes_profile_lifecycle.rs:108` (`hermes_profile_directory_exists`), `hermes_home()`, `hermes_profile_dir()` | Existing path resolution — reuse, do not duplicate |
| `managed_agents/runtime_types.rs:90` (`local_setup`) | Additive sibling field for the named state |
| `shared/api/types.ts:296` (`localSetup`) | TS mirror |
| `shared/lib/configNudge.ts:69` | TS mirror of the new variants |

## Work

1. **RED contract tests** (write first, watch fail, record output):
   - Each of the five states produced from a fixture: healthy profile →
     `ready` + `auth-unknown` advisory; deleted dir → `missing`;
     corrupt `config.yaml` → `broken-config`; binary off PATH →
     `binary-missing`.
   - `auth-unknown` on a healthy profile does **not** produce any
     `Requirement` and does **not** yield `AgentReadiness::NotReady`
     (the DD-1 regression guard — this is the single most important
     test in the phase).
   - Each blocking state carries human-readable, actionable copy.
   - Tests use a temp `HERMES_HOME` and `lock_path_mutex()`, matching
     the three existing tests in `readiness/hermes.rs`.

2. **Extend `Requirement`** (upstream file, ~+25 lines): two additive
   variants for broken config and unrunnable binary, alongside the
   existing `HermesProfileDirectoryMissing` and `MissingBinary`.
   Additive only — no restructuring, no reordering, no restyling
   (`UPSTREAM-SYNC.md` § Thin-fork rules). Check whether the existing
   `MissingBinary { command }` (`readiness.rs:328`) already covers
   `binary-missing` for the ambient case; if it does, reuse it rather
   than adding a variant, and record that in the PR.

3. **Extend the evaluator** in `readiness/hermes.rs` (Crew-owned):
   config parse per the phase-01 decision, cached binary probe per
   phase-01 TTL, existing directory check. Doc-comment the file as
   Hermes-specific per D-025. No `unwrap()`/`expect()`; no `unsafe`.

4. **Advisory channel:** add the non-blocking readiness field to
   `runtime_types.rs` (~+4 lines) and mirror in `types.ts` (~+3 lines).
   It carries the named state including `ready` and `auth-unknown`,
   plus the copy for honest degradation.

5. **Mirror the new variants** in `configNudge.ts` (~+18 lines), one
   arm each, following the `hermes_profile_directory_missing` pattern
   at `:69`.

6. **`docs/crew/STATE.md`** updated in this PR (#117 anti-drift,
   plan DD-8).

## Files

- **Modify (upstream, justified):** `managed_agents/readiness.rs`,
  `managed_agents/runtime_types.rs`, `shared/api/types.ts`,
  `shared/lib/configNudge.ts`
- **Modify (Crew-owned):** `managed_agents/readiness/hermes.rs`
- **Read only:** `hermes_profile_lifecycle.rs`, `discovery.rs` (PATH
  augmentation), phase-01 spike record
- **Must not touch:** `AcpRuntimeCatalogEntry` / runtime catalog,
  `lib/agentConfigCore.ts` (DD-2), the create flow, occupancy checks,
  owner-only/local invariants (issue non-goals)

## Validation

- All RED tests green; the DD-1 guard test explicitly asserted.
- `cargo test --manifest-path desktop/src-tauri/Cargo.toml` green (the
  root workspace does not run desktop tests — `AGENTS.md` gotcha 5).
- `pnpm exec tsc --noEmit` clean in `desktop/`.
- `node desktop/scripts/check-file-sizes.mjs` passes — if a touched file
  nears `MAX_LINES = 1000`, extract Crew deltas into Crew-owned files
  (D-022). Never raise the limit, never add an override.
- `just ci` green.
- Manual: a healthy Hermes agent still spawns into a working session,
  not setup mode.

## Risk and rollback

- **Risk:** an over-eager config check reports `broken-config` on a
  working profile and blocks it. Mitigation: phase-01's conservative
  decision, plus the requirement that every blocking state is
  recoverable from the nudge without dropping to a terminal.
- **Risk:** binary probe latency on every readiness read. Mitigation:
  phase-01 TTL; invalidate at the post-install re-evaluation points
  (`commands/agent_discovery.rs:295-330`, `:425-455`).
- **Rollback:** the new variants are additive; reverting the evaluator
  restores the directory-only check. No data migration, no persisted
  state.

## PR

Branch `agents/profile-readiness` (product area, not phase number).
Target `Nuncio-hq/crew` (D-020). `git commit -s`. PR body records the
RED failure output, the upstream diff sizes actually produced against
the plan's estimates, and the D-025 generic/Hermes split.
