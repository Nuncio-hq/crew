# Tester Report — Thread Worktree UI and Crew Release Namespace

Date: 2026-07-31  
Worktree: `/Users/a1241968/Desktop/Oscar/LilGroup/Nuncio/.worktrees/crew-thread-worktree-ui`  
Branch: `agent/thread-worktree-ui`  
Result: PASS

## Scope

Final independent integration verification on the exact current uncommitted
bytes after review fixes. Production code was not edited. This report is the
only tester-authored file.

## Results

### Backend and telemetry

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check -p buzz-acp --all-targets` | PASS |
| `cargo clippy -p buzz-acp --all-targets -- -D warnings` | PASS |
| `cargo test -p buzz-acp` | PASS: 661 library tests + 9 integration tests; 0 failed |

The final suite includes exact-root authority checks, missing-root fail-closed
telemetry, independent concurrent thread creation, same-root convergence,
same-prefix collision protection, legacy worktree adoption, and concurrent
legacy adoption safety.

Final verified backend hashes:

- `thread_workspace.rs`: `ae1b1c2280a6dbc5acd643f89867171ae04a947945bc733892041ada5d04fcc5`
- `thread_workspace_tests.rs`: `94337fca30069b5b4db47699b57ae2e6f16277629fdbf275b3137c821161f011`
- `pool.rs`: `2717860702d0d9a2037e593d21e0eff961194476bd31b85479ee66584e10d086`

### Desktop projection and UI

| Command | Result |
|---|---|
| `pnpm check` | PASS: Biome, file-size, px-text, and pubkey-truncation guards |
| `pnpm typecheck` | PASS |
| `pnpm test` | PASS: 3,885 passed, 0 failed, 1 existing environment-gated skip |
| `pnpm build:e2e` | PASS: 4,572 modules transformed |
| `pnpm exec playwright test tests/e2e/project-thread-worktree.spec.ts --project=smoke` | PASS: 2 passed |

The unit suite verifies out-of-order frame rejection, deterministic equal-time
watermarks, per-community save/restore without leakage, bounded LRU eviction,
and root-scoped active-agent snapshots.

The Playwright flow verifies:

- one Project channel can display distinct thread worktrees without state
  cross-contamination;
- the normal Slack-like composer remains available;
- agent handoff rows remain scoped to the open root;
- a failed workspace renders failed truth without a preparing spinner or
  shared-workspace affordance.

Fresh capture hashes are distinct:

- `01-workspace-ready.png`:
  `79a768b4b97de7ce51501e570b74a4c2b8a3350b8c9a87e1dc77bdda1968d152`
- `02-full-project-thread.png`:
  `7ede45a383903c404a404fe147b0f875c1be007f1c175aa86665f87d634b0505`

The broad-suite skip is the existing real-relay environment-gated Project
link/relink test; it is not a failure.

### Crew release namespace

| Command | Result |
|---|---|
| `node --import ./test-loader.mjs --experimental-strip-types --test src/testing/nuncio-crew-release-contract.test.mjs` | PASS: 12 passed, 0 failed |
| `node scripts/nuncio-crew-release-channel.mjs v0.0.5 stable aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa` | PASS |

Verified output keeps the semantic updater version at `0.0.5`, publishes the
immutable release under `crew-v0.0.5`, advances stable and dev rolling tags,
and points the stable updater endpoint at the Nuncio-owned repository.

### Final hygiene

| Command | Result |
|---|---|
| `git diff --check` | PASS |

## Unresolved Questions / Blockers

None.

**Status:** DONE  
**Summary:** Exact final bytes pass Rust format/check/clippy/tests, full desktop checks and unit tests, focused Playwright UI coverage, release contracts, and diff hygiene.  
**Concerns/Blockers:** None.
