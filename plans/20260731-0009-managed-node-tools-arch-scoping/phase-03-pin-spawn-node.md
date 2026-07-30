# Phase 03 — Pin adapter spawn to the managed Node

## Overview

- **Priority:** High
- **Status:** Complete (verified-PATH + loud fail; no spawn-time download)
- Depends on: phase 01
- Remove the silent fallback that lets install-arch and run-arch diverge.

## Context

The crash in the issue reports `Node.js v26.0.0` (Homebrew, arm64) while the
managed runtime is `v24.18.0`. Mechanism, verified:

Managed bin dirs are appended to `PATH` **unconditionally, without checking that
they exist** — `managed_agents/runtime/path.rs:116-122`,
`managed_agents/discovery.rs:31-36`, and `build_augmented_path` in
`runtime/path.rs`. The adapter shim `bin/codex-acp` starts with
`#!/usr/bin/env node`. When the arch-scoped managed node dir is absent (the
state after an arch flip, since `ensure_managed_node_runtime_blocking()` runs
only in the install flow at `agent_discovery.rs:342` and never at spawn),
`env node` walks past the empty managed entry and resolves Homebrew's Node.

So the adapter is executed by a Node that did not install it. That is what turns
"wrong optional dependency" into a hard crash rather than a warning.

This is independent of the prefix scoping. Phase 01 and 02 do not fix it.

## Requirements

1. When spawning an ACP adapter, resolve Node explicitly to
   `buzz_managed_node_bin_path()` rather than relying on `PATH` order. Invoke the
   adapter's JS entry point with that Node, or set the child's `PATH` such that
   the managed node dir is both present *and verified to contain a node binary*.

   Prefer explicit invocation over PATH ordering — PATH ordering is what failed.

2. If the managed Node is expected (i.e. the adapter was installed into the
   managed prefix) but is missing or fails its `--version` probe, fail with a
   clear, Buzz-specific error naming the repair. Do not fall through to system
   Node. A loud failure at spawn is strictly better than the current silent
   arch divergence.

3. Adapters that are *not* managed (user-installed vendor CLIs found on the
   system PATH) keep their current resolution. This phase must not change how a
   Homebrew-installed `codex` is discovered — only how a managed adapter is run.
   `buzz_managed_command_path()` (`managed_node_paths.rs:63`) already
   distinguishes managed from unmanaged; use it as the discriminator rather than
   inventing a new one.

4. Consider calling `ensure_managed_node_runtime_blocking()` on the spawn path,
   not only the install path. Weigh it: it can block a spawn on a 90 MB download.
   If it is called, it must be behind the readiness check so the common case is a
   cheap `--version` probe, and the download must not run on a UI thread.

   If the implementer judges the blocking download unacceptable at spawn time,
   requirement 2's loud failure is sufficient for this phase — say so explicitly
   in the PR rather than doing it silently.

## Files

- `desktop/src-tauri/src/managed_agents/runtime/path.rs` — stop contributing
  nonexistent managed dirs to `PATH`, or mark them so the caller can tell.
- `desktop/src-tauri/src/managed_agents/discovery.rs:31-36` — same question for
  `common_binary_paths()`.
- The ACP adapter spawn site. Find it before editing:
  `grep -rn "codex-acp\|claude-agent-acp" desktop/src-tauri/src crates/buzz-acp`.
  Do not guess the location from this document.

## Steps

1. Locate the actual spawn path and read it. Report what you find before
   changing it — this phase's file list is a starting point, not a survey.
2. Decide explicit-invocation vs verified-PATH and justify it in the PR.
3. Implement, with the unmanaged-adapter path provably unchanged.

## Tests

- a managed adapter resolves to the managed Node path when it exists;
- a managed adapter with the managed Node absent produces a Buzz-specific error,
  not a system-Node spawn;
- an unmanaged adapter resolves exactly as it does today (regression guard);
- `build_augmented_path` / `compose_path_entries` no longer emit a managed dir
  that does not exist — and still emit every other entry in the same order.

The PATH-composition functions have existing tests and documented invariants
about ordering (`runtime/path.rs` has a long comment explaining why the composed
PATH is set twice, and why login-shell PATH is skipped on Windows). Read that
comment before touching them. Preserve every stated invariant; if one must
change, call it out.

## Success criteria

- An adapter is never executed by a Node other than the one that installed it,
  or the attempt fails loudly.
- Windows behaviour unchanged (login_shell_path is always `None` there — the
  existing comment explains why; do not regress it).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `./scripts/run-tests.sh unit` passes.

## Risk and rollback

This phase touches PATH composition, which is load-bearing for *all* agent
discovery, not just the managed ones. It is the highest-regression-risk phase in
the plan and the reason it is sequenced after the two that close the defect.

If it destabilises discovery, ship phases 01, 02 and 04 without it and reopen
this phase separately. That is an acceptable outcome — say so rather than
forcing it through.
