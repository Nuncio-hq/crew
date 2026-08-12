Title: Complete Hermes Doctor readiness and dependency diagnostics

## Problem

PR #134 supplies the Hermes readiness evaluator and named states, but the
remaining Doctor contract still lacks compatibility, ACP dependency, MCP
availability, safe diagnostics, and read-only update discovery
(`desktop/src-tauri/src/managed_agents/hermes_profile_readiness.rs:20-39,59-80,87-149`
on PR #134 / branch `agents/profile-lifecycle-hardening`).

## What to solve

Implement the approved R1 delta from
[`phase-01-doctor-completion.md`](../phase-01-doctor-completion.md):

- Run spike **S-A** first for parseable `hermes --version` and truthful
  `hermes acp --check` semantics.
- Run spike **S-C** first for side-effect-free `hermes update --check`.
- Add the minimum-compatible-version gate and `Incompatible` state only after
  S-A passes.
- Probe `hermes acp --check` and `buzz-dev-mcp` with bounded, sanitized output.
- Add explicit re-check, copyable repair guidance, and an allowlisted
  diagnostics contract.
- Preserve honest `AuthUnknown`; do not infer auth from text or ship an auth
  badge.

The MCP probe covers the documented failure class where a missing MCP path
produces `auth error: BUZZ_PRIVATE_KEY is required`
(`docs/crew/HERMES.md:163-167` on main).

## Definition of Done

- [ ] S-A and S-C have PASS/FAIL/INCONCLUSIVE results recorded before implementation.
- [ ] RED contracts DR-01…DR-11 fail before implementation and pass afterward.
- [ ] Incompatible, ACP dependency, MCP availability, refresh, repair-copy,
      update-discovery, and redaction behaviors are observable.
- [ ] Existing #134 readiness states remain compatible.
- [ ] Auth remains `AuthUnknown`; no auth badge or text-derived auth claim ships.
- [ ] Focused Rust/desktop tests, desktop typecheck, and docs checks pass.

## Evidence required

- A reproducible spike record for S-A and S-C.
- RED/GREEN test evidence for each DR contract.
- Sanitized screenshots or presenter assertions for every actionable state.
- No raw environment, credentials, or profile secrets in evidence.

## Non-goals

No Hermes auth probe or auth badge; no Hermes upstream edit; no profile
export/import; no model, memory, skills, credentials, plugin, cron, gateway,
webhook, or raw-config editor; no automatic update/install; and no change to
`STATE.md`, `DECISIONS.md`, or `HERMES.md`.

## Dependencies

- PR #134 must merge before this issue starts.
- Spike S-A gates compatibility and ACP checks.
- Spike S-C gates update discovery.
