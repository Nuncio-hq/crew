---
phase: 03
title: Spawn preflight and attention routing
status: planned
priority: P0
effort: M (2 d)
dependencies: ["02"]
---

# Phase 03 — Spawn preflight and attention routing

## Outcome

Detectable profile breakage never becomes a silent stall. When readiness
is blocking, the owner learns *why* through the surfaces they already
watch — attention and Needs-You — instead of watching an agent sit
quiet.

DoD coverage: #2.

## What already exists (do not rebuild)

`desktop/src-tauri/src/managed_agents/runtime.rs:~575-630` **already**
evaluates readiness at spawn: on `AgentReadiness::NotReady` it builds
`BUZZ_ACP_SETUP_PAYLOAD` (`{agent_name, agent_pubkey, requirements}`),
sets `spawned_setup_mode`, and `buzz-acp` runs as a setup listener that
publishes a kind:9 `buzz:config-nudge` fenced sentinel, rendered by
`config-nudge-attachment.tsx`.

Phase 02 makes that path fire for three states instead of one. This
phase closes the two real gaps (plan DD-3):

- **Gap A:** the nudge lands in the channel, but nothing routes it into
  the agent's attention state or Needs-You. An owner not reading that
  channel still sees an agent that "just doesn't answer".
- **Gap B:** breakage that appears *after* a healthy spawn (profile
  deleted mid-turn, binary removed) is never re-detected.

## Seams

| Seam | Use |
| ---- | --- |
| `managed_agents/runtime.rs` setup-payload branch (~:575-630) | The single hook point (~+15 lines); logic in Crew files |
| `commands/agent_discovery.rs:295-330`, `:425-455` (`should_restart_after_install`) | The existing model for re-evaluating readiness after the fact — copy the shape, not the code |
| `desktop/src/features/agents/agentAttention.ts` | Named attention states (`AGENTS.md` rule 3 — named reasons, not booleans) |
| `desktop/src/features/agents/needsYouStore.ts`, kinds 46010/46040 | Owner-visible item with TTL |
| `shared/lib/configNudge.ts` `extractConfigNudge()` | Already parses the payload; reuse as the routing input |

## Work

1. **RED contract tests** first (DD-7), failure output in the PR body:
   - Spawn with a blocking readiness state produces an attention state
     with the named reason, not a generic "not responding".
   - The attention item's copy is actionable and names the repair
     (recreate / rebind / install / fix config), not just the fault.
   - A healthy spawn produces **no** attention item and **no**
     Needs-You entry — the false-positive guard.
   - `auth-unknown` alone never produces an attention item (DD-1
     regression guard at the routing layer).
   - Re-evaluation: an agent healthy at spawn whose profile disappears
     mid-turn transitions to the blocking state at the next trigger
     rather than hanging.

2. **Gap A — attention routing.** In a Crew-owned module, map the
   existing config-nudge / requirement signal onto an attention state
   and, where the owner must act, a Needs-You item. Keyed on the
   *presence and kind of requirement*, never on `runtime.id` (D-025) —
   a future engine emitting requirements routes identically with no
   change here.

3. **Gap B — re-evaluation.** Implement the trigger phase 01 recommended
   (default: turn boundary — cheapest, no new machinery, no watcher
   handles). Re-run the phase-02 evaluator; on a transition to blocking,
   route as in Gap A. Respect the cached binary probe TTL so this does
   not become a per-turn process spawn.

4. **`runtime.rs` hook** (upstream, ~+15 lines): the smallest possible
   call into the Crew-owned routing at the existing branch. Do not
   restructure the surrounding function.

5. **`docs/crew/STATE.md`** updated in this PR (#117).

## Files

- **Modify (upstream, justified):** `managed_agents/runtime.rs`
- **Create (Crew-owned):** readiness→attention routing module under
  `desktop/src/features/agents/` (or its Rust-side counterpart if the
  spike places the trigger in Rust) + its tests
- **Modify (Crew-owned):** attention/Needs-You wiring as required
- **Read only:** `commands/agent_discovery.rs`, `configNudge.ts`,
  phase-02 output
- **Must not touch:** the setup-payload contract itself, `buzz-acp`
  setup-listener behavior, the kind:9 sentinel format — all generic
  Buzz contracts that other engines depend on

## Validation

- RED tests green, including both false-positive guards.
- Fault-injection by hand: delete a bound profile directory mid-turn →
  the agent surfaces the named state at the next trigger instead of
  going quiet; restore it → the state clears without a restart.
- `cargo test --manifest-path desktop/src-tauri/Cargo.toml`, desktop
  unit tests, `pnpm exec tsc --noEmit`, `just ci` — all green.
- Confirm no component gained a `runtime.id` branch (D-025 /
  `features/agents/AGENTS.md` rule 1).

## Risk and rollback

- **Risk:** attention noise. A profile that is briefly unreadable
  (editor writing `config.yaml`) must not spam Needs-You. Mitigation:
  route on a *stable* transition, not on every read; Needs-You entries
  are deduplicated per agent per state.
- **Risk:** per-turn re-evaluation adds latency to every turn.
  Mitigation: cached probe; directory check is a cheap stat.
- **Rollback:** revert the `runtime.rs` hook — the setup-mode path
  returns to its current shipped behavior, which is functional, just
  quieter.

## PR

Branch `agents/profile-preflight`. Target `Nuncio-hq/crew` (D-020).
`git commit -s`. PR body records the RED output and the fault-injection
transcript for the mid-turn case.
