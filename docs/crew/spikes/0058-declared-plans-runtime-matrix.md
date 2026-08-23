---
title: "Declared Plans Runtime Matrix"
tags: [declared-plans, acp, runtime-matrix]
status: active
created: 2026-08-23
---

# Spike 0058 — Declared plans runtime matrix

- **Status:** PASS (in-repo evidence + unit matrix tests; live re-capture deferred per runtime)
- **Date:** 2026-08-23
- **Related:** D-056 / #190, spike [0025](0025-acp-plan-live-wire.md)

## Question

Which Crew ACP runtimes emit plan state the Declared Plans rail can show, and what
signal shape does each use?

## Runtime matrix

| Runtime | Spawn command | Plan signal | Rail after projection fix |
| --- | --- | --- | --- |
| Hermes | `hermes` / `hermes-acp` | Native `sessionUpdate: plan` `entries[]` (documented; Hermes `todo` → plan translation per #190) | Yes — native ACP plan |
| Codex | `codex-acp` | Native `sessionUpdate: plan` `entries[]` (in-repo parser + upstream changelog) | Yes — native ACP plan |
| Claude Code | `claude-agent-acp` | Native plan unknown; `TodoWrite` tool with `todos[]` `{content, status}` (desktop test) | Yes — todo-tool fallback |
| Cursor | `cursor-agent` | `CreatePlan` plan-gate tool; native `sessionUpdate: plan` on updates **unverified** | Yes when native plan frames arrive; CreatePlan without `todos[]` stays unknown |
| Buzz Agent | `buzz-agent` | buzz-dev-mcp `todo` `{text, done}` | Yes — todo-tool fallback |
| Goose | `goose` | No captured native plan; harness `todo` if MCP wired | Todo fallback only when structured `todos[]` present |
| Grok / xAI | custom harness | **Zero** `sessionUpdate: plan` in spike 0018; plan mode edits `plan.md` via `search_replace` | **Plan unknown** (by design — no markdown scrape) |

Catalog: `desktop/src-tauri/src/managed_agents/discovery/known_runtimes.rs`.
Cursor: `desktop/src-tauri/src/managed_agents/cursor_startup_model.rs`.

## Desktop fixes landed (2026-08-23)

1. **`declaredPlanProjection.ts`** — timestamp-first plan authority (fixes stale
   rail after agent `seq` reset on process restart).
2. **`useDeclaredPlansForThread.ts`** — `liveSessionId` from merged
   live+archived observer window.

## Test contract

`desktop/src/features/agents/declaredPlanRuntimeMatrix.test.mjs` pins:

- Native ACP wholesale replace (Hermes/Codex shape)
- Claude `TodoWrite` rows
- Buzz `todo` `{text, done}`
- `tool_call_update` without `todos` does not invent or clobber snapshots
- Grok `plan.md` `search_replace` → not a plan
- `CreatePlan` without structured todos → not a plan
- Markdown-only `sessionUpdate: plan` → not a plan

## Follow-up (live capture)

Re-verify on a real machine:

1. Cursor — multiple `sessionUpdate: plan` frames vs `CreatePlan`-only
2. Hermes — todo → native plan translation on each update
3. Codex — plan entries on plan-mode turns

If Cursor never emits native plan on update, add adapter normalization or
`CreatePlan` arg parser **only after** capturing structured `rawInput`.

## Verdict

**PASS.** Projection bug fixed for all runtimes that emit structured ACP plan or
todo tool frames. Grok plan.md and prose markdown correctly remain unknown.
