# Phase 02 — Crew UI: binding, readiness, no-model guard (Slice 2)

- **Status:** Not started — requires manager approval of the slice plan
  plus RED contracts before implementation (D-008)
- **Contracts:** C-03, C-04, C-05, C-06, C-10, C-12 (see feature doc §8)

## Deliverable

Crew-side UX that makes the D-019 conventions enforced instead of
documented:

1. **Profile binding field** on create/edit when the runtime identity is
   Hermes — pick an existing profile (from `hermes profile list`) with
   the manager's default profile excluded (P-7).
2. **Model/provider suppression** (C-04): read-only "provided by profile
   *X* — currently *model*" sourced from the session models catalog;
   no editable field.
3. **Spawn guard** (C-05/C-06, spike 0013): a last-write
   `env_remove("BUZZ_ACP_MODEL")` for Hermes-runtime records, additive
   Crew module + minimal call-site in `runtime.rs`, hash-consistent with
   `spawn_config_hash`.
4. **Readiness classes** (C-03/C-12 degraded): binary missing / profile
   missing / model unconfigured, each with a distinct actionable
   message. (Auth badge stays blocked on the Hermes probe ask.)
5. **Duplicate-binding guard** (C-10): warn-or-block when binding a
   profile already bound to a live agent.

## RED-first test contract

Every numbered item lands with failing tests first:

- Rust: spawn-env assertions (model absent for Hermes across field
  resolution and env maps; goose/claude unaffected — C-15), spawn-hash
  invariance, readiness classification.
- TS/Playwright (mock bridge, `pnpm build:e2e`): editor renders
  read-only model row for Hermes; binding field flows; duplicate-bind
  warning.

## Thin-fork budget

- New Crew-owned module(s) for the guard + binding storage.
- Upstream-file edits limited to: persona/agent editor surface, create
  flow, one `runtime.rs` call site — each with the "why composition is
  insufficient / expected diff size" justification in this file before
  implementation, per the FEATURE template.

## Dependencies / ordering

- Spike 0013 (enforcement point) — done.
- Phase 01 not strictly required, but if the upstream entry has landed,
  the runtime-identity check reuses `known_acp_runtime` instead of a
  fork-local matcher — prefer waiting for at least the upstream PR shape
  to be fixed.

## Exit criteria

All six contracts GREEN; `just ci` passes; verification record with a
live create → assign → reply pass through the desktop app UI.
