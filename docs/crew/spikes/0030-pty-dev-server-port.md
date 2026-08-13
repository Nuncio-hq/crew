# Spike 0030 — PTY dev server `$PORT` + readiness (#196)

- **Status:** PASS (unit; live PTY not required in CI)
- **Date:** 2026-08-13
- **Issue:** [#196](https://github.com/Nuncio-hq/crew/issues/196)

## Question

Can Crew own a channel/worktree dev server as a **labeled Buzz Term PTY
session** with `$PORT` injection and a readiness probe (`readyPattern`
or port bind) before the Browser tab attaches?

## Decision affected

D-058 — no new process manager. Governor allocates a free port, wraps
`buzz-terminal`, restarts on crash (max 3), idle-reaps per policy.

## Hypothesis

`terminal_attach` already opens a fenced PTY. The governor can attach a
labeled session, write the expanded command, and watch output / TCP for
readiness. `$PORT` is string-substituted before input; a busy requested
port falls through to the next free port (conflict note).

## Scope

- Pure functions: port pick, command expansion, readyPattern match,
  crash-restart counter
- Fake PTY at the process boundary in tests
- Live `buzz-terminal` attach is the production path; CI does not spawn
  a real Vite server

## Pass criteria

1. `pnpm dev --port $PORT` expands to a numeric port.
2. Readiness is true when output matches `readyPattern` or the port
   accepts a connection.
3. Three crashes mark the holding `crashed` (last log lines kept).
4. Idle with no webview / no requests / no agent activity schedules
   stop with a visible countdown.

## Fail criteria

A second process manager, or attaching the webview before ready.

## Verdict

**PASS** via unit tests in `resource_governor` (`port.rs`,
`dev_server.rs`). Production attach uses existing `terminal_attach` /
`terminal_input`.
