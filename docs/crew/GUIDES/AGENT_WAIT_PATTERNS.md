---
title: "Agent Wait Patterns"
tags: [agents, ci, harness, runbook]
status: active
created: 2026-08-23
---

# Agent wait patterns

Agents that block in one turn on CI, deploys, or human review often appear to
"die" mid-wait. That is usually **not** a single runtime bug — several
independent clocks punish silent in-turn blocking.

## The rule: checkpoint, then resume

**Publish state and end the turn before waiting on anything expected to take more
than ~2 minutes.** Waiting is a **new wake**, not continuation of the same turn.

### Do

1. Push / open PR / start the external job.
2. Post channel status with the PR link and what you are waiting on.
3. **End the turn** — do not hold it open.
4. Resume on wake via:
   - human `@mention`
   - `/loop` (monitored shell or subscription timer)
   - Buzz workflow webhook → `send_message` with `@Agent` (see
     [CI webhook wake](../examples/CI_WEBHOOK_WAKE_SETUP.md))
   - scheduled workflow poll (last resort)

### Don't

- Run `gh pr checks --watch` or `gh run watch` in the **same turn** as push
  without ending the turn first.
- Use a single long `sleep` (or huge `block_until_ms`) without publishing and
  ending the turn.
- Assume the harness will stay alive indefinitely — it won't.

## Why agents die (clocks)

| Layer | Default | Config | Symptom |
|-------|---------|--------|---------|
| ACP idle timeout | **900s (15 min)** | `BUZZ_ACP_IDLE_TIMEOUT` | Turn cancelled; may retry or post failure notice |
| ACP hard cap | **7200s (2h)** | `BUZZ_ACP_MAX_TURN_DURATION` | Dead-letter or `⚠️ couldn't process…` message |
| Pool spin-down (lazy) | **1800s (30 min)** | `BUZZ_ACP_POOL_IDLE_TIMEOUT` | Engine subprocess torn down; cold start on next mention |
| Desktop stall UI | 90s / 30s | — | "Possibly stalled" / "Lost contact" (turn often actually cancelled) |

Idle timeout resets only on **ACP JSON stdout**, not on shell progress. A silent
`gh pr checks --watch` does not reset the idle clock.

Key code: `crates/buzz-acp/src/acp.rs` (`read_until_response_with_idle_timeout`),
`crates/buzz-acp/src/config.rs` (defaults).

## If you must poll in-turn

Rare — prefer checkpoint-and-resume. When unavoidable:

- Poll in **30–60s** chunks, not one long watch.
- Post brief channel progress between chunks.
- Keep total silent stretch under `BUZZ_ACP_IDLE_TIMEOUT` (default 900s).

## Config escape hatch (operator)

Raising timeouts is a band-aid; checkpoint-and-resume is still preferred.

| Setting | When | Where |
|---------|------|-------|
| `idle_timeout_seconds` | CI often exceeds 15 min | Agent record → `BUZZ_ACP_IDLE_TIMEOUT` |
| `max_turn_duration_seconds` | Full ship pipeline > 2h | Agent record → `BUZZ_ACP_MAX_TURN_DURATION` |
| `BUZZ_ACP_POOL_IDLE_TIMEOUT=0` | Keep engine warm during long channel silence | Lazy harness env (`desktop/src-tauri/src/managed_agents/agent_env.rs`) |

## Diagnostic checklist

When an agent dies mid-CI:

1. Harness log — `IdleTimeout`, `HardTimeout`, or `pool idle timeout reached`
2. Channel — `⚠️ couldn't process the last request` failure notice?
3. Desktop — `possibly-stalled` / `lost-contact` / `sleeping`
4. Agent config — `idle_timeout_seconds`, lazy vs eager pool
5. Command — `gh pr checks --watch`, `sleep`, `Await`, etc.
6. Elapsed time — compare to 900s / 1800s / 7200s

## Related

- Example workflow: [`examples/ci-complete-wake-workflow.yaml`](../examples/ci-complete-wake-workflow.yaml)
- Setup: [`examples/CI_WEBHOOK_WAKE_SETUP.md`](../examples/CI_WEBHOOK_WAKE_SETUP.md)
- Attention model spike: [`spikes/0014-agent-attention-recovery.md`](../spikes/0014-agent-attention-recovery.md)
- Harness config: `crates/buzz-acp/src/config.rs`

## Follow-up (not yet shipped)

- Harness `known-wait` observer events (UI grace exists; emission does not —
  spike 0014 gap)
- `buzz ci watch` CLI helper (background poll → wake message)
