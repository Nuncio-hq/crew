# Idle engine spin-down with resume-first wake — upstream proposal draft

> Draft artifact held under D-020. This is not a submitted pull request against
> `block/buzz`.

## Summary

Local ACP harnesses already defer engine startup (`BUZZ_ACP_LAZY_POOL`) but
never reverse `Listening → Waking → Ready`. Idle engines accumulate RSS across
agents × communities. Upstream should land the Crew-free core:

1. **Pool lifecycle reverse path** — `Ready → Draining → Listening` with sleep
   eligibility (no in-flight turn, empty queue, flushed outboxes) and
   `BUZZ_ACP_POOL_IDLE_TIMEOUT` (default 30m, `0` = off). Heartbeat must not
   wake `Listening`.
2. **Durable session ledger** — declare session ids at successful
   `session/new`; resume by lookup only; validate engine identity + workspace
   generation + `loadSession` before `session/load`; fail closed to rebuild.
3. **`session/load` client** — ACP v1 request (`sessionId`, `cwd`,
   `mcpServers`) gated on initialize capabilities.

Crew-only surface (Sleeping card / Mission Inbox exclusion) stays in the fork.

## Why upstream

The seams (`PoolLifecycle`, `SessionState.sessions`, secure_spool,
`WorkspaceBinding`) are Buzz-owned. Desktop already forces lazy pool for local
pairs. Without upstream spin-down, every Buzz desktop user pays permanent
engine RSS after the first mention.

## Non-goals (same as Crew #169)

- Compaction-awareness / lineage history UI
- Memory-pressure eviction
- Multi-slot resume (`BUZZ_ACP_AGENTS > 1`)
- Full harness exit (`BUZZ_ACP_EXIT_AFTER_INACTIVITY` unchanged)
- Per-thread process isolation

## Evidence already in Crew

- Spike 0022 `loadSession` reality matrix (Hermes/Codex usable-resume;
  buzz-agent rebuild-only)
- Contract tests: ledger rules, lifecycle drain/rewake, `session_load` wire
- Desktop Sleeping UI projection (fork-only)

## Suggested upstream landing order

1. Ledger module + declare-at-birth (restart survival alone is valuable)
2. Spin-down lifecycle + timeout flag
3. Resume-first wake + capability gate
