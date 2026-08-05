# Phase 02 — Crew UI: binding, readiness, no-model guard (Slice 2)

- **Status:** Phase 02A (Rust core) COMPLETE — UI half still pending
- **Contracts:** C-03, C-04, C-05, C-06, C-10, C-12 (see feature doc §8)
- **PR scope this commit:** Rust only (plus TS catalog `profileArg` mirror)

## Deliverable

Crew-side UX that makes the D-019 conventions enforced instead of
documented:

1. **Profile binding field** on create/edit when the runtime identity is
   Hermes — pick an existing profile (from `hermes profile list`) with
   the manager's default profile excluded (P-7).
   → **02A done (storage + validation):** `ManagedAgentRecord.hermes_profile`,
   create/update request fields, `validate_hermes_profile_name`, duplicate
   binding guard (C-10). UI picker is Phase 02B.
2. **Model/provider suppression** (C-04): read-only "provided by profile
   *X* — currently *model*" sourced from the session models catalog;
   no editable field. → **02B (TS)**.
3. **Spawn guard** (C-05/C-06, spike 0013): a last-write
   `env_remove("BUZZ_ACP_MODEL")` for Hermes-runtime records, additive
   Crew module + minimal call-site in `runtime.rs`, hash-consistent with
   `spawn_config_hash`. → **02A DONE** (`hermes_profile.rs` +
   `runtime.rs` call after `descriptor.env` loop; `spawn_hash` excludes
   model / strips env key for Hermes).
4. **Readiness classes** (C-03/C-12 degraded): binary missing / profile
   missing / model unconfigured, each with a distinct actionable
   message. (Auth badge stays blocked on the Hermes probe ask.)
   → **02A DONE (degraded):** `MissingBinary` + `NormalizedField {
   hermesProfile }`. Profile-directory-on-disk check deferred (follow-up;
   Goose has home-dir file reading precedent but a new
   `~/.hermes/profiles/<name>` probe was not invented here). Auth probe
   still unavailable (spike 0010).
5. **Duplicate-binding guard** (C-10): warn-or-block when binding a
   profile already bound to a live agent. → **02A DONE (server reject)**
   on create/update for same `relay_url` scope.

## Design decisions locked in 02A (A–F)

| ID | Decision |
| -- | -------- |
| A | `KnownAcpRuntime.profile_arg: Option<&'static str>` — only Hermes is `Some("-p")`. Projected as `AcpRuntimeCatalogEntry.profile_arg` / TS `profileArg`. One-rule compliant. |
| B | `ManagedAgentRecord.hermes_profile: Option<String>` (serde default/skip). Validate `^[a-z0-9][a-z0-9_-]{0,63}$`; reject `"default"`. |
| C | Args injection in `resolve_effective_harness_descriptor`: prepend `[flag, name]` before normalized args; skip if flag already present; ignore binding when runtime has no `profile_arg`. |
| D | `strip_model_env_for_profile_locked_runtime` after last user-env write; hash uses post-guard view. |
| E | Hermes readiness: binary + bound profile. No dir existence / no auth probe. |
| F | Duplicate binding: server-side reject on create/update (same relay). |

## RED-first test contract

Every numbered item lands with failing tests first:

- Rust: spawn-env assertions (model absent for Hermes across field
  resolution and env maps; goose/claude unaffected — C-15), spawn-hash
  invariance, readiness classification. → **DONE (02A)**; RED evidence
  captured before flipping `profile_arg` to `Some("-p")`.
- TS/Playwright (mock bridge, `pnpm build:e2e`): editor renders
  read-only model row for Hermes; binding field flows; duplicate-bind
  warning. → **02B**.

## Thin-fork budget

- New Crew-owned module: `managed_agents/hermes_profile.rs`.
- Upstream-adjacent edits: `KnownAcpRuntime` / catalog entry / `runtime.rs`
  one call site / `spawn_hash.rs` / `readiness.rs` / record + requests —
  each required for the one-rule capability fact or spawn-funnel guard.

## Dependencies / ordering

- Spike 0013 (enforcement point) — done.
- Phase 01 Hermes tier-1 entry landed on Crew `main` (D-020).

## Exit criteria

- 02A: Rust contracts C-03 (binding), C-05, C-06, C-10, C-12 (degraded)
  GREEN; desktop lib tests + clippy/fmt + typecheck pass.
- Full phase: All six contracts GREEN including C-04 UI; live create →
  assign → reply pass through the desktop app UI.
