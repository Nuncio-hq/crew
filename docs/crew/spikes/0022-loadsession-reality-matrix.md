# Spike 0022 — `loadSession` reality matrix (idle spin-down resume)

- **Status:** PASS (narrowed)
- **Date:** 2026-08-12
- **Issue:** [#169](https://github.com/Nuncio-hq/crew/issues/169)

## Question

For Hermes ACP, claude-code-acp / claude-agent-acp, Codex ACP, and buzz-agent:
does `initialize` advertise usable `loadSession`? Does `session/load` restore
enough transcript to survive engine process death? Is the session id
disk-keyed?

## Decision affected

Issue #169 part 3 (resume-first wake). If no engine we actually run supports
usable resume, parts 1–2 (spin-down + ledger + rebuild) still ship and resume
narrows to engines that pass.

## Hypothesis

Hermes and Codex advertise `loadSession: true` with disk-backed session stores.
buzz-agent honestly advertises `false`. Claude's ACP adapter likely advertises
true but may prune/cleanup. Resume is therefore engine-gated, not universal.

## Scope

- Providers: Hermes ACP, Codex ACP, claude-agent-acp, buzz-agent
- Evidence sources: prior authenticated spike assets under
  `docs/crew/spikes/assets/0018-spawn-granularity/`, buzz-agent source, ACP
  protocol docs for `session/load` params
- Bound: no overnight RSS capture in this cloud VM; no live authenticated
  message-13 recall run when engines/credentials are absent

## Exclusions

- Compaction / cleanup-period endurance beyond what prior probes recorded
- Multi-slot (`BUZZ_ACP_AGENTS > 1`) resume
- ACP v2 `session/resume` migration (harness remains on v1 `session/load`)

## Pass criteria

Per engine: a clear verdict (`usable-resume` / `rebuild-only` / `unknown`) with
cited evidence for (a) capability advertisement, (b) disk vs RAM session id,
(c) whether #169 may call `session/load` for that engine.

## Fail criteria

No engine has enough evidence to decide, blocking the whole epic — **not**
triggered if at least one engine is `usable-resume` or all are honestly
`rebuild-only` (parts 1–2 still ship).

## Environment

- Commit: Crew `main` at spike authoring time (see PR)
- Prior assets: spike 0018 real-engine JSON probes (Hermes xAI OAuth, Hermes
  OpenAI Codex path, Codex ACP, Grok)
- buzz-agent: `crates/buzz-agent/src/lib.rs` initialize capabilities
- Protocol: ACP v1 `session/load` requires `sessionId`, `cwd`, `mcpServers`

## Method

1. Inspect `initialize.agentCapabilities.loadSession` in spike 0018 assets.
2. Inspect buzz-agent capability advertisement in source.
3. Note sessionCapabilities (`resume` / `list`) when present.
4. Confirm buzz-acp has no `session/load` client path today (greenfield).
5. Record ACP request shape for the implementation contract tests.

## Results

| Engine | Advertises `loadSession` | Session id survives process death | Verdict |
| --- | --- | --- | --- |
| Hermes ACP (`hermes-agent`) | **true** — `real-hermes-*-two-session-probe.json` also shows `sessionCapabilities.resume/list/fork` | Yes — Hermes persists under `~/.hermes/profiles/<profile>/`; prior spikes treat session ids as durable | **usable-resume** |
| Codex ACP (`@agentclientprotocol/codex-acp`) | **true** — `real-codex-two-session-probe.json`; `sessionCapabilities.resume/list/close/delete` | Yes — Codex rollout/thread store is disk-backed; session ids are UUIDs returned from `session/new` | **usable-resume** |
| Claude ACP (`@agentclientprotocol/claude-agent-acp`) | Not present in authenticated 0018 assets in this checkout | Claude Code writes session JSONL; cleanup period may prune | **unknown → rebuild-until-proven** (gate on live `loadSession: true` + successful load; never assume) |
| buzz-agent | **false** — `crates/buzz-agent/src/lib.rs` + README | N/A — no `session/load` | **rebuild-only** |

Wire shape (ACP v1), for harness implementation:

```json
{
  "method": "session/load",
  "params": {
    "sessionId": "<ledger.current.session_id>",
    "cwd": "/absolute/path",
    "mcpServers": []
  }
}
```

Clients MUST NOT call `session/load` unless `agentCapabilities.loadSession`
is true.

## Edge cases observed

- ACP v2 removes `session/load` in favor of `session/resume` + `replayFrom`.
  Crew/Buzz harness still requests protocol v1/v2 pin in `initialize`; v1
  `session/load` remains the correct call for current adapters that advertise
  `loadSession`.
- Hermes probes show session provenance metadata (`sessionProvenance`) — useful
  for later compaction-awareness, out of scope for #169.
- buzz-acp today never reads `loadSession` and has no `session/load` method —
  resume is entirely additive.

## Limitations

- This cloud environment did not re-run live authenticated engine probes;
  Hermes/Codex capability rows reuse prior authenticated spike 0018 JSON.
- Claude verdict is intentionally fail-closed (`unknown`) until a live probe
  records `loadSession` + post-death load success.
- Message-13 recall (fact beyond `context_message_limit`) remains an
  integration Verify item for engines marked usable-resume, not proven here.

## Verdict

**PASS (narrowed).** Hermes ACP and Codex ACP are `usable-resume` targets.
buzz-agent is the canonical `rebuild-only` fallback engine. Claude stays
capability-gated at runtime (rebuild unless initialize advertises true and
`session/load` succeeds). Parts 1–2 of #169 ship for all engines; part 3
calls `session/load` only when the capability gate and ledger validation pass.

## Follow-up test contract

RED before implementation:

1. Ledger contract: declare-at-birth / resume-by-lookup-only /
   validate-then-load; corrupt file → absent; overwrite-on-rotation; no secrets
   in file.
2. Lifecycle: Ready → Draining → Listening only when sleep-eligible; heartbeat
   does not wake Listening; buffered work during Draining re-wakes after drain.
3. ACP client: `session_load` sends `sessionId` + `cwd` + `mcpServers`.
4. Wake path: `loadSession: false` → rebuild; identity/workspace mismatch →
   delete ledger entry + rebuild; load rejection → same.
5. Multi-thread: wake via thread A resumes A's session id, never B/C's.

## Cleanup

No temporary processes or credentials created for this spike. Evidence remains
in existing `docs/crew/spikes/assets/0018-spawn-granularity/` JSON files plus
this record.
