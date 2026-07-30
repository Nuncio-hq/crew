# Phase 02 — Validate arch at readiness, self-repair, deterministic install

## Overview

- **Priority:** Critical
- **Status:** Complete
- Depends on: phase 01
- Make a mismatched adapter tree a detected cache miss that repairs itself,
  instead of an undetected state that fails at agent spawn.

## Context

`managed_node_runtime_ready()` (`managed_node.rs:105-124`) probes only
`node --version` against the arch-specific path. Nothing inspects `node-tools/`
at any point, so a tree containing the wrong platform packages is indistinguishable
from a correct one until an agent dies at spawn. Phase 01 makes a mismatch land
on a *different path*, which turns most of this into a miss — but a tree can still
be corrupt within its own platform dir (interrupted install, partial npm failure),
and the legacy unscoped dir still occupies disk.

## Requirements

1. **Adapter arch validation.** Before declaring adapters ready, confirm the
   installed platform package matches `std::env::consts::ARCH`. The concrete
   check: under `<npm_prefix>/lib/node_modules/`, the vendored optional
   dependency directory for each installed adapter ends with the expected
   platform suffix — `@openai/codex-<platform>` and
   `@anthropic-ai/claude-agent-sdk-<platform>`, where `<platform>` is
   `managed_platform_segment()` from phase 01.

   Derive the package names from the adapter catalog rather than hardcoding two
   literals if the catalog exposes them; if it does not, hardcode but keep the
   list in one named constant next to the check.

   Treat "no adapters installed at all" as **not a mismatch** — that is the
   normal pre-install state and must not trigger a purge.

2. **Purge and reinstall on mismatch.** On mismatch, remove
   `<npm_prefix>` (the platform-scoped dir only) and let the existing install
   path rebuild it. Purge must:
   - only ever remove a path that is inside `data_dir()/<product>/node-tools`;
     assert this before calling `remove_dir_all` — a bug in path resolution must
     not become a recursive delete of something else;
   - never touch `runtimes/`;
   - on failure, return a failed `InstallStepResult` with a hint naming the
     directory, following the shape of `managed_node_failed_step`. No panic, no
     silent continue.

3. **Reclaim the legacy unscoped tree.** If
   `buzz_legacy_unscoped_npm_prefix()` exists and contains
   `lib/node_modules`, remove it — once, opportunistically, and only after a
   successful scoped install so a failed migration never leaves the user with
   nothing. Failure to remove is a log line, not an error: it is disk hygiene,
   not correctness.

4. **Deterministic install.** Add `--cpu=<arch> --os=<os>` to the rewritten npm
   command in `rewrite_npm_global_install()` (`managed_node.rs:~500`), mapping
   Rust's `std::env::consts::ARCH` to npm's vocabulary (`aarch64` → `arm64`,
   `x86_64` → `x64`) and `OS` to npm's (`macos` → `darwin`, others pass through).
   Verify the npm-side spelling against the npm docs before committing —
   getting this wrong silently installs nothing.

   Apply it to `install`/`i`, not to `uninstall`.

## Files

- Modify `desktop/src-tauri/src/commands/agent_discovery/managed_node.rs` —
  arch validation, purge, `--cpu`/`--os`.
- Modify `desktop/src-tauri/src/commands/agent_discovery.rs` — call the
  validation in the install flow around `:342`, before
  `ensure_managed_node_runtime_blocking`.
- Consider `desktop/src-tauri/src/commands/agent_discovery/post_install_verification.rs`
  — it already runs a post-install availability probe and is the natural place to
  assert arch correctness *after* an install too. Reuse it rather than adding a
  parallel verification path.

## Steps

1. Write failing tests against directory fixtures.
2. Implement `managed_adapter_arch_matches(prefix: &Path) -> ArchCheck` as a
   pure function over a directory tree so it is testable without an install.
   Return a three-state result (`Matches | Mismatch | NoAdaptersInstalled`), not
   a `bool` — the bool version cannot express requirement 1's third case.
3. Implement the guarded purge.
4. Wire both into the install flow.
5. Add the legacy cleanup after a successful install.
6. Add `--cpu`/`--os`.

## Tests

- fixture with `@openai/codex-darwin-x64` on a host reporting `aarch64` →
  `Mismatch`;
- fixture with the matching platform package → `Matches`;
- empty prefix / no `lib/node_modules` → `NoAdaptersInstalled`, and assert no
  purge is attempted;
- purge removes `node-tools/<platform>` and leaves a sibling `runtimes/`
  directory present;
- purge refuses (returns error, removes nothing) when handed a path outside the
  node-tools root;
- purge failure yields a failed `InstallStepResult` carrying a hint;
- `rewrite_npm_global_install` emits `--cpu` and `--os` for `install` and `i`,
  and omits them for `uninstall`;
- all six existing `rewrite_npm_global_install` / `shell_quote` tests still pass.

Arch is a compile-time constant, so the arch-matching function must take the
expected platform as a parameter to be testable. Do not read
`std::env::consts::ARCH` inside the pure function.

## Success criteria

- On arm64 with only an x64 tree present, the app detects the mismatch, purges,
  reinstalls, and agents start — without user intervention.
- No path outside `node-tools` is ever removed, proven by test.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `./scripts/run-tests.sh unit` passes.

## Risk and rollback

The purge is the dangerous part of this whole plan. The guard in requirement 2 is
not optional and its test is not optional. If the reviewer is unconvinced by the
guard, that blocks the phase.

Rollback: revert; the app returns to failing at spawn, which is the status quo.
