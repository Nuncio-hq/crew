# Phase 01 — Arch-scope the npm prefix, de-hardcode the product dir

## Overview

- **Priority:** Critical
- **Status:** Complete
- Make the npm prefix carry the same platform segment the Node runtime already
  carries, from a single shared source of truth, and stop hardcoding `"Buzz"`.

## Context

`buzz_managed_npm_prefix()` (`managed_node_paths.rs:3`) returns
`data_dir()/Buzz/node-tools` with no platform segment, while
`buzz_managed_node_bin_dir()` (`managed_node_paths.rs:13`) appends
`<version>/<platform>`. npm resolves optional platform packages from the
`process.arch` of the npm that runs, so an x64 npm writes `darwin-x64` binaries
into a path an arm64 build will later read as its own.

## Requirements

1. Extract the `(os, arch) -> platform` match from `buzz_managed_node_bin_dir()`
   into one shared function, e.g.:

   ```rust
   pub(crate) fn managed_platform_segment() -> Option<&'static str>
   ```

   It returns `darwin-arm64 | darwin-x64 | linux-x64 | linux-arm64 | win-x64 |
   win-arm64`, or `None` for an unsupported pair. `buzz_managed_node_bin_dir()`
   keeps its own `bin_subdir` decision (Windows has no `bin/`) but must take the
   platform string from this function — not from a second inline match.

2. `buzz_managed_npm_prefix()` becomes
   `data_dir()/<product>/node-tools/<platform>`, returning `None` when the
   platform is unsupported. Today it returns `Some` on every platform; callers
   that treat `None` as "cannot resolve app-data dir" already emit a correct
   failure step (`managed_node.rs:477`), so returning `None` on an unsupported
   platform is a behaviour change in the right direction — but check each caller.

3. Add a legacy accessor for the unscoped path, used only by phase 02's cleanup:

   ```rust
   pub(crate) fn buzz_legacy_unscoped_npm_prefix() -> Option<PathBuf>
   ```

4. Replace the hardcoded `"Buzz"` in **both** `buzz_managed_npm_prefix()` and
   `buzz_managed_node_root()` with a resolved product dir:

   ```rust
   static PRODUCT_DIR: OnceLock<String> = OnceLock::new();
   pub(crate) fn set_managed_product_dir(name: String);   // called once at startup
   fn managed_product_dir() -> &'static str;              // falls back to "Buzz"
   ```

   Seed it during Tauri setup from the app config identifier. The identifier is
   injected at build time via `TAURI_CONFIG`
   (`tauri.nuncio-crew-release.conf.json:3` → `com.nuncio.crew`), so it is only
   available through the `AppHandle` at runtime.

   Derive a filesystem-safe directory name from the identifier. Decide and
   document the mapping — the obvious choice is the product name if the config
   exposes it, else the last identifier segment. Whatever is chosen, it must be
   stable across releases of the same product: changing it later orphans every
   user's tree.

   `set_managed_product_dir` must be idempotent-safe: a second call with a
   different value is a bug, so use `OnceLock::set` and log (do not panic) if it
   was already set.

## Files

- Modify `desktop/src-tauri/src/managed_agents/managed_node_paths.rs` — all of
  the above.
- Modify the Tauri setup entry point (`desktop/src-tauri/src/lib.rs` or
  `main.rs`, whichever holds the `setup` closure) to call
  `set_managed_product_dir` before any managed-agent work runs. It must run
  before discovery, readiness, and install — verify the ordering, do not assume it.
- Modify `desktop/src-tauri/src/managed_agents/mod.rs` if the new symbols need
  re-export (existing callers reach these through
  `crate::managed_agents::buzz_managed_*`).
- No change expected in `discovery.rs`, `runtime/path.rs`, or
  `agent_discovery.rs` — they consume the accessors. Confirm by compiling, and
  if a change is needed, say so rather than widening silently.

## Steps

1. Write the failing tests first (see below).
2. Extract `managed_platform_segment()`; rewire `buzz_managed_node_bin_dir()` to
   use it. Confirm no behaviour change: same paths as before for every pair.
3. Add the platform segment to `buzz_managed_npm_prefix()`.
4. Add `buzz_legacy_unscoped_npm_prefix()`.
5. Add the product-dir `OnceLock` + seeding, with the `"Buzz"` fallback.
6. Audit every caller of the three accessors (10 call sites, listed by
   `grep -rn "buzz_managed_npm_prefix\|buzz_managed_npm_bin_dir\|buzz_managed_node_root"`)
   for `None`-handling and for assumptions about the old layout.

## Tests

In `managed_node_paths.rs`:

- all six `(os, arch)` pairs map to the expected platform string;
- an unsupported pair yields `None` from both `managed_platform_segment()` and
  `buzz_managed_npm_prefix()`;
- **the npm prefix and the node bin dir agree on the platform segment** — assert
  the relationship, not two string literals, so the two cannot drift again;
- `buzz_managed_npm_prefix()` ends with `node-tools/<platform>`;
- `buzz_legacy_unscoped_npm_prefix()` ends with `node-tools` and has no platform
  segment;
- `managed_product_dir()` returns `"Buzz"` when unseeded.

Note the `OnceLock` makes seeded/unseeded a per-process property, so a test that
seeds it will affect others in the same binary. Either keep the seeding test in
its own integration test, or make the fallback testable through a pure helper
that takes the value as a parameter. Prefer the pure helper.

## Success criteria

- `cargo check --manifest-path desktop/src-tauri/Cargo.toml` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- New tests pass; every existing test in `managed_node.rs` still passes.
- A fresh install on arm64 writes to `node-tools/darwin-arm64/`.
- Stock Buzz and NuncioCrew resolve to different product dirs at runtime.

## Risk and rollback

Changing the prefix orphans every existing user's adapter install — they get one
reinstall on next launch. That is the intended cost and it is bounded: the
install path already exists and already works. Phase 02 reclaims the disk.

Rollback is a single-commit revert; no data migration to undo. Do not add one.
