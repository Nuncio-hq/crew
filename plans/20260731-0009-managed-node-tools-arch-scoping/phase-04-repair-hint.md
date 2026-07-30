# Phase 04 — Buzz-specific repair hint for the upstream error text

## Overview

- **Priority:** Medium
- **Status:** Complete
- Depends on: phase 01 (needs the scoped prefix to name in the hint)
- Stop handing users advice that cannot work.

## Context

The error users actually see comes from upstream Codex:

```
Error: Missing optional dependency @openai/codex-darwin-arm64.
Reinstall Codex: npm install -g @openai/codex@latest
```

`npm install -g` targets the *global* npm prefix. Buzz installs into its own
private prefix (`managed_node.rs:477` rewrites `-g` to
`--prefix <buzz-private-path>`). So following the printed advice changes nothing,
and the user has no way to discover the real repair. Same text appears for
`@anthropic-ai/claude-agent-sdk-*`.

There is an existing precedent for this pattern: `npm_eacces_hint()`
(`managed_node.rs:~560`) inspects stderr for a known upstream failure string and
returns Buzz-specific guidance. Follow that shape — do not invent a new
mechanism.

## Requirements

1. Detect `Missing optional dependency @(openai/codex|anthropic-ai/claude-agent-sdk)-`
   in adapter stderr and attach a Buzz-specific hint.

2. The hint must name the actual repair available in the app ("Reinstall
   adapters from Settings", or whatever the real affordance is — check the UI
   before writing the string) rather than a shell command the user must
   reconstruct. If a shell command is included as a fallback, it must use the
   resolved private prefix from phase 01, not a hardcoded path, and must not
   embed the user's home directory literal into a shared string.

3. After phase 02, this state should be self-healing — the hint is the safety net
   for the case where automatic repair failed. Word it accordingly: it is not the
   primary path.

## Files

- `desktop/src-tauri/src/commands/agent_discovery/managed_node.rs` — add the
  detector next to `npm_eacces_hint`.
- The ACP spawn-failure surface that renders adapter stderr to the user. Locate
  it (`grep -rn "agent initialize failed" crates/buzz-acp desktop/src-tauri/src`)
  and confirm hints are actually rendered there before wiring it up. If adapter
  stderr never reaches a hint-capable surface, say so — that is a finding, and
  the phase becomes "route it there first".

## Tests

- the detector matches the Codex message and the claude-agent-sdk message;
- it does not match unrelated stderr (guard against a hint on every failure);
- the returned hint mentions the in-app repair;
- the returned hint does not contain the literal string `npm install -g`.

## Success criteria

- A user hitting this failure is told something that works.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `./scripts/run-tests.sh unit` passes.

## Risk and rollback

Low. Additive string matching on a failure path. Rollback is a revert.

The one real risk is an over-broad matcher attaching a misleading hint to
unrelated adapter failures — hence the negative test.
