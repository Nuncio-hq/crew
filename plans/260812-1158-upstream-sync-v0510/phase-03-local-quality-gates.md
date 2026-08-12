---
phase: 3
title: "Local quality gates"
status: pending
priority: P1
effort: "1-2h"
dependencies: [2]
---

# Phase 3: Local quality gates

## Overview

Prove the merged tree compiles and passes Crew-local fast checks before
spending Actions minutes. Prefer Hermit-activated `just` recipes.

## Requirements

- Functional: workspace + desktop Tauri + desktop JS unit/lint green locally (or documented blockers).
- Non-functional: no secret commits; fmt/clippy clean enough for hooks.

## Related Code Files

- Touch only if gates fail: conflict leftovers, baselines, import errors from Phase 2

## Implementation Steps

1. Hermit: `. ./bin/activate-hermit`

2. Rust workspace (root):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib
```

If full workspace is too heavy for the machine, minimum:

```bash
cargo check --workspace
cargo test -p buzz-acp --lib
cargo test -p buzz-cli --lib
```

3. Desktop Tauri:

```bash
cargo fmt --manifest-path desktop/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path desktop/src-tauri/Cargo.toml
```

4. Desktop JS:

```bash
cd desktop
pnpm lint
pnpm exec tsc --noEmit
pnpm test
pnpm check:px-text   # if present in package scripts
```

5. File-size / thin-fork guards Crew already runs:

```bash
# from repo root / via just if wired
just desktop-check   # or the scripts used by NuncioCrew Gate
```

Fix D-022 baseline bumps only for upstream-grown files.

6. Optional focused regression for Crew seams:

```bash
cd desktop && pnpm exec vitest run src/features/messages   # if unit tests exist there
cargo test -p buzz-cli evidence -- --nocapture 2>/dev/null || true
```

## Success Criteria

- [ ] No compile errors in root workspace or `desktop/src-tauri`
- [ ] Desktop lint + `tsc --noEmit` pass
- [ ] Desktop unit tests pass
- [ ] File-size guard pass (or intentional upstream baseline updates committed)
- [ ] Known failures documented with owner before Phase 4 (none preferred)

## Risk Assessment

| Risk | Mitigation |
|---|---|
| Worktree path breaks Tauri fmt | Use main checkout for fmt only |
| Clippy noise from upstream | Fix real errors; avoid broad allows |

## Next Steps

Phase 4 — push branch, run Upstream Sync workflow, smoke evidence/agents/perf.
