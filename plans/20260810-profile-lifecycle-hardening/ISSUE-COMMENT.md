# Plan ready — 7 phases

Plan: `plans/20260810-profile-lifecycle-hardening/plan.md`.
Validate **pass**; red-team **9 findings, 8 applied, 1 rejected with rationale**.
All 6 DoD checkboxes map to ≥1 phase (mapping table in the plan).

| # | Phase | Effort | Deps |
| - | ----- | ------ | ---- |
| 01 | Spike 0015 — readiness signals and archive mechanics | S (0.5-1 d) | — |
| 02 | Readiness model, evaluator, and projection | M (2-3 d) | 01 |
| 03 | Spawn preflight and attention routing | M (2 d) | 02 |
| 04 | Archive, restore, permanent delete, running-agent guard | L (3-4 d) | 01 |
| 05 | Readiness surfacing — card and dialog | M (2 d) | 02 |
| 06 | Offboarding archive, restore, permanent-delete UI | L (3 d) | 04, 05 |
| 07 | Verification, fault injection, and docs | M (2 d) | 03, 06 |

02→05 ships the readiness half, 04→06 the archive half — independently
shippable, each leaving `main` green.

## Key design decisions

- **`auth-unknown` is not a `Requirement`.** A `Requirement` implies `NotReady`,
  which makes `runtime.rs` spawn into setup-listener mode. With no headless auth
  probe (spike 0010), *every* healthy profile is `auth-unknown` — modelling it as
  blocking would take the runtime down. Split: `missing`/`broken-config`/
  `binary-missing` block; `ready`/`auth-unknown` are advisory. All five names
  still reach the UI.
- **Carrier correction.** The issue names `AcpRuntimeCatalogEntry` /
  `agentConfigCore.ts`; per `features/agents/AGENTS.md` rule 1 those hold
  harness-scoped capability facts and field descriptors. Per-agent, time-varying
  readiness rides the per-agent `Requirement` pipeline + runtime status
  projection instead. Intent preserved (no frontend rival table, no `runtime.id`
  checks, named reasons from Rust); mechanism corrected.
- **The graceful stop machinery the issue cites does not exist.**
  `runtime/stop.rs:40,120,153` uses `Child::kill()` (immediate SIGKILL); the only
  `SIGTERM` in `managed_agents` is `discovery.rs:927`, in the auth-probe timeout.
  The guard is **refuse-while-running**, authoritative in Rust — not "archive
  stops the agent for you".
- **The "existing backups area" does not exist either** — no `NuncioCrew Backups`
  anywhere in the repo. Location/format is spike 0015's first output; no phase
  hardcodes a path before then.
- **RED is a gate inside each phase**, not its own PR (a tests-only PR can't merge
  green). **STATE.md is updated by every shipping phase** (#117), not just 07.
- **Thin fork:** 12 upstream files, ≈ **+153 / -24 lines**, every edit a hook or a
  type mirror; substantive logic in Crew-owned files. Per-file justification in
  the plan. **D-025:** transport, preflight, attention routing and the card stay
  generic; YAML parse, `hermes --version`, profile-dir layout and archive/restore
  are labelled Hermes-specific and confined to Crew files.

## Named Buzz seams

`readiness.rs:284/:333/:340` · `readiness/hermes.rs` · `runtime.rs:~575-630`
(`BUZZ_ACP_SETUP_PAYLOAD` setup-mode divert) · `configNudge.ts:69` +
`config-nudge-attachment.tsx` · `runtime_types.rs:90` → `types.ts:296` →
`managedAgentRuntimeStatus.ts:12` (the boolean being replaced) ·
`AgentStatusBadge.tsx` / `ManagedAgentRow.tsx` · `agentAttention.ts` /
`needsYouStore.ts` (46010/46040) · `hermes_profile_lifecycle.rs`
(`HermesProfileLifecycleResult`) · `commands/hermes_profiles.rs` +
`lib.rs:795-797` · `hermes_profile.rs:13,19` (`default` reject) ·
`usePersonaActions.ts:297-317` (the irreversible delete) ·
`agent_discovery.rs:295-330,:425-455` (post-install re-evaluation model).

## Open questions (phase 01 resolves 1-4)

1. Archive location — none exists; app-data-dir proposed.
2. Archive format — `tar.gz` (new crate) vs plain copy (no dependency).
3. Which `config.yaml` conditions honestly mean `broken-config`; whether an
   invalid model id is locally detectable at all.
4. Re-evaluation trigger — turn boundary (default), fs watch, or timer.
5. DECISIONS.md entry for archive semantics — recommended yes (D-028, phase 07).
