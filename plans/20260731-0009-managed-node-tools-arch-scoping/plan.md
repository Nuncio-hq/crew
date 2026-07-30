# Managed node-tools arch scoping and self-repair

- **Status:** Implemented — awaiting review
- **Date:** 2026-07-31
- **Issue:** Nuncio-hq/crew#4

## Goal

Make the managed Node/npm tree impossible to leave in a permanently broken state
after an architecture flip, and stop forks from sharing one poisoned tree. After
this work, an arch mismatch is a cache miss that self-heals, not a dead end.

## Problem (verified against source)

`buzz_managed_npm_prefix()` is arch-agnostic while `buzz_managed_node_bin_dir()`
is arch-scoped, so the two trees can disagree and nothing ever notices.

| Fact | Location |
|---|---|
| npm prefix has no platform segment: `data_dir()/Buzz/node-tools` | `desktop/src-tauri/src/managed_agents/managed_node_paths.rs:3` |
| node bin dir is arch-scoped: `…/runtimes/node/<version>/<platform>/bin` | `managed_node_paths.rs:13` |
| both roots hardcode the literal `"Buzz"` — forks collide | `managed_node_paths.rs:4`, `:10` |
| `MANAGED_NODE_ARTIFACT` is `#[cfg(target_arch)]` — an arm64 build cannot even name the x64 tree it must clean up | `commands/agent_discovery/managed_node.rs:18-70` |
| `managed_node_runtime_ready()` probes only `node --version`; `node-tools/` is never validated | `managed_node.rs:105-124` |
| `ensure_managed_node_runtime_blocking()` is called only from the install flow, never at agent spawn | `commands/agent_discovery.rs:342` |
| install prefix comes from the unscoped path (`--prefix`, `NPM_CONFIG_PREFIX`, `NPM_CONFIG_CACHE`, `COREPACK_HOME`) | `managed_node.rs:477`, `agent_discovery.rs:893` |

### Why the crash reports Node v26 and not the managed v24

Managed bin dirs are pushed onto `PATH` **unconditionally, whether or not they
exist** (`managed_agents/runtime/path.rs:116-122`,
`managed_agents/discovery.rs:31-36`, `runtime/path.rs` `build_augmented_path`).
After an arch flip the arm64 node dir is absent, so the `#!/usr/bin/env node`
shebang in `bin/codex-acp` walks past the empty managed entry and lands on
Homebrew's Node. Install-arch and run-arch then diverge silently. This is the
mechanism behind Defect C in the issue and it is a *separate* bug from the
prefix scoping — fixing the prefix alone leaves the silent PATH fallthrough.

## Scope

1. [Arch-scope the npm prefix and de-hardcode the product dir](phase-01-arch-scoped-prefix.md)
2. [Validate arch at readiness and self-repair](phase-02-readiness-and-repair.md)
3. [Pin adapter spawn to the managed Node](phase-03-pin-spawn-node.md)
4. [Buzz-specific repair hint for the upstream error text](phase-04-repair-hint.md)

Phases 1 and 2 close the defect. Phase 3 removes the mechanism that makes it a
hard failure instead of a warning. Phase 4 is the user-facing dead end.

## Design decisions

**Layout.** `data_dir()/<Product>/node-tools/<platform>/`, using the same
`<platform>` vocabulary already produced by `buzz_managed_node_bin_dir()`
(`darwin-arm64`, `darwin-x64`, `linux-x64`, `linux-arm64`, `win-x64`,
`win-arm64`). Extract that `(os, arch) -> platform` match into one shared
function and call it from both path builders — the whole class of bug is that
two functions independently decide what "platform" means.

**No migration of the existing tree.** An arch flip becomes a cache miss that
triggers a clean install. Moving `node-tools/` into `node-tools/<platform>/`
would preserve exactly the corrupt state we are trying to shed, and we cannot
know which arch produced it. Leave the legacy dir in place; phase 2 garbage
collects it. Do not attempt an in-place rename.

**Product dir.** The bundle identifier is injected at build time via
`TAURI_CONFIG` (`tauri.nuncio-crew-release.conf.json:3` → `com.nuncio.crew`),
so there is no compile-time constant available to a free function. Resolve it
once at startup into a `OnceLock<String>` seeded from the Tauri `AppHandle`
config, with the literal `"Buzz"` as the fallback when unset (tests, headless
callers). Do **not** thread `AppHandle` through `managed_node_paths.rs` — every
caller is a free function and that refactor is out of scope.

Preserve `"Buzz"` as the fallback deliberately: an unseeded call must not
silently invent a third tree.

**`--cpu` / `--os` on install.** Pass `--cpu=<arch> --os=<os>` to the npm
install so the result stops depending on which Node happens to run npm. This is
belt-and-braces once the prefix is scoped, but it is the difference between "the
wrong tree is in the wrong place" and "the wrong tree cannot be built".

## Locked boundaries

- No change to `MANAGED_NODE_VERSION` or the pinned artifact SHA256s.
- No change to the archive download/extract/validation path — the zip and tar
  traversal guards in `managed_node.rs` stay exactly as they are.
- No new dependencies.
- Do not delete anything outside `data_dir()/<Product>/node-tools*`. Never touch
  `runtimes/`, identity, or agent config.
- Purge must be fail-safe: if removal fails, surface an `InstallStepResult` with
  a hint. Never panic, never silently continue as if the tree were clean.

## Success criteria

- On an arm64 machine with only a `darwin-x64` tree present, first launch of the
  arm64 build installs a clean `darwin-arm64` tree and all agents start.
- Stock Buzz and NuncioCrew installed side by side never read or write each
  other's `node-tools`.
- An adapter is always spawned by the managed Node when the managed Node exists;
  when it does not, that is an explicit failure, not a silent fallback.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `./scripts/run-tests.sh unit` and `cargo check --manifest-path desktop/src-tauri/Cargo.toml` pass.

## Verification

Unit tests are the primary gate — the arch-flip scenario is reproducible by
constructing directory fixtures, and must not require an actual Rosetta run.

Required cases:
- platform segment resolution for all six `(os, arch)` pairs, and `None` for an
  unsupported pair;
- npm prefix and node bin dir agree on the platform segment (same input → same
  segment, asserted directly rather than by string literal);
- readiness returns not-ready when the installed platform package does not match
  `std::env::consts::ARCH`;
- purge removes only `node-tools/<platform>` (and the legacy unscoped dir) and
  leaves `runtimes/` untouched;
- purge failure produces a failed `InstallStepResult` with a hint, not a panic;
- product dir falls back to `"Buzz"` when the `OnceLock` is unseeded;
- the existing `rewrite_npm_global_install` tests still pass with the scoped
  prefix, and a new case asserts `--cpu`/`--os` are present.

Manual smoke on macOS arm64, in this order:
1. `mv "$HOME/Library/Application Support/Buzz/node-tools" /tmp/node-tools.bak`
   (do not delete — it is the reproduction fixture).
2. Launch NuncioCrew, install adapters, confirm agents start.
3. Confirm `node-tools/darwin-arm64/lib/node_modules/.../codex-darwin-arm64` exists.
4. Restore the x64 tree as the legacy unscoped dir and confirm the app treats it
   as a miss and reinstalls rather than failing at spawn.

## Out of scope

- #2690 (unversioned `Buzz.app.tar.gz` shared across darwin arches). It is the
  likely route onto the x64 build but it is a release-packaging fix, not this one.
- Any change to `buzz repos` / relay git surfaces.
