# Phase 1 — Worktree registry + durable thread strip

Depends on: Phase 0.5 (`origin_repo_target`, shipped in `19220ade3`).
Delivers: one read-only Tauri command that reconstructs every thread workspace
from disk + GitHub, and rewires the thread strip to fall back to it. No new UI.

## Why first

`projectThreadWorkspaceStore.ts` is fed only by live observer events
(`thread_workspace_ready` / `_error`) and holds an in-memory `Map` (cap 256,
`projectThreadWorkspaceStore.ts:33`). Restart the app and every strip reverts to
`pending`, although the worktree, branch and PR all still exist on disk. Phase 2
and Phase 3 both need enumeration that does not depend on a running agent.

## Design: derive, don't remember

Everything needed is already deterministic
(`crates/buzz-acp/src/thread_workspace.rs:86-95`):

```
short  = root_event_id[..12]
branch = buzz/<short>
path   = <repo_root_parent>/.buzz-worktrees/<repo_name>-<short>
```

and the reverse mapping is persisted by `record_branch_root` as
`branch.<branch>.buzzThreadRoot` in the repo's local git config.

## Files

| File | Change |
| --- | --- |
| `desktop/src-tauri/src/commands/project_worktree_registry.rs` | **new** — command + types + assembly |
| `desktop/src-tauri/src/commands/project_worktree_registry_parse.rs` | **new** — porcelain/config parsers + unit tests |
| `desktop/src-tauri/src/commands/project_worktree_registry_github.rs` | **new** — single `gh pr list` fetch + branch map |
| `desktop/src-tauri/src/commands/mod.rs` | register modules (`mod` + `pub use`, near lines 58-61 / 116-117) |
| `desktop/src-tauri/src/lib.rs` | add `get_project_worktree_registry` to the handler list (near line 667) |
| `desktop/src/shared/api/thread-workspace-types.ts` | add registry types |
| `desktop/src/shared/api/agentControl.ts` | add `getProjectWorktreeRegistry()` |
| `desktop/src/features/agents/projectWorktreeRegistryStore.ts` | **new** — TTL cache + `useSyncExternalStore` |
| `desktop/src/features/agents/useProjectThreadWorkspace.ts` | registry fallback when the observer snapshot is `pending` |
| `desktop/src/features/agents/projectThreadWorkspaceStore.ts` | add the `derived` snapshot variant |
| `desktop/src/features/messages/ui/ProjectThreadWorkspacePanel.tsx` | accept `derived` for the Workspace cell and for `target` |
| `desktop/src/features/communities/useCommunityInit.ts` | reset the new store (mandatory, see line 75 pattern) |

## Command contract

```rust
#[tauri::command]
pub async fn get_project_worktree_registry(
    repository_path: String,
) -> Result<ProjectWorktreeRegistry, String>;
```

```rust
struct ProjectWorktreeRegistry {
    repository_path: String,     // canonical repo root
    managed_root: String,        // <repo_parent>/.buzz-worktrees
    github: GithubAvailability,  // available | unavailable
    entries: Vec<ProjectWorktreeEntry>,
}

struct ProjectWorktreeEntry {
    worktree_path: String,
    worktree_name: String,
    branch: Option<String>,          // None = detached HEAD
    head: String,
    kind: ProjectWorktreeKind,       // main | managed | external
    root_event_id: Option<String>,
    prunable: bool,                  // porcelain reported the dir as gone
    pull_request: Option<RegistryPullRequest>,
}

struct RegistryPullRequest {
    number: u64, state: String, is_draft: bool, review_decision: String,
    checks: RegistryChecksState,     // passing | failing | pending | none
    additions: u64, deletions: u64, title: String, url: String,
}
```

Deliberately **absent**: `dirty`, `ahead/behind`, `disk_bytes`. Those cost one
process per worktree (and `du` on 18 GB); they are per-row lazy in Phase 3.

## Assembly steps

1. Canonicalize `repository_path`, then `git -C <path> rev-parse --show-toplevel`
   and `--git-common-dir` — same normalization `ensure_thread_worktree` performs,
   so a subdirectory path still resolves. Reuse
   `thread_workspace_git::{git_output_at, git_output_dir}` (already has the 20 s
   timeout + `kill_on_drop`).
2. `git --git-dir <common> worktree list --porcelain` → split on blank lines;
   read `worktree`, `HEAD`, `branch refs/heads/…`, `detached`, `prunable`, `bare`.
3. Classify each entry per D1: `main` for the primary worktree; `managed` when
   the canonical parent equals `<repo_parent>/.buzz-worktrees` **and** the branch
   matches `^buzz/[0-9a-f]{12}$`; `external` otherwise.
4. `git --git-dir <common> config --get-regexp '^branch\..*\.buzzthreadroot$'`
   → branch → root event id.
   **Gotcha (verified):** git lowercases the key in output —
   `branch.buzz/eb791333c0ee.buzzthreadroot`. Both cased and lowercased regexes
   match (4 rows each on this repo), but the *parser* must be case-insensitive:
   strip the `branch.` prefix and the trailing `.buzzthreadroot`, keeping the
   middle verbatim (branch names may contain `.`, so split on the last segment).
   Accept only 64-hex values; ignore anything else.
5. `origin_repo_target(&repo_root)` (Phase 0.5) → `gh pr list --repo <target>
   --state all --limit 100 --json headRefName,number,state,isDraft,reviewDecision,statusCheckRollup,additions,deletions,title,url`,
   one call for the whole repo. Map by `headRefName`; keep the open PR when a
   branch has several, else the most recent. Reduce `statusCheckRollup` to a
   single `RegistryChecksState` with the existing `parse_check` logic
   (`thread_github.rs:152-170`) — extract it rather than duplicating.
6. Any `gh` failure (missing binary, unauthenticated, no origin) →
   `github: unavailable`, all `pull_request: None`. Never fail the command: the
   git half must still render.

## Frontend

`projectWorktreeRegistryStore.ts` mirrors `projectThreadGitHubStore.ts`
(module `Map` + epoch counter + `useSyncExternalStore` + TTL), **not** React
Query — the brainstorm suggested React Query, but the neighbouring store already
solves the same problem this way and the reset contract is wired for it.

- key: canonical repository path
- TTL 60 s; `refresh(force)` for manual reload
- invalidate on `thread_workspace_ready` / `_error` observer events (they drop
  from source-of-truth to cache-invalidation signal)
- exported selector `getProjectWorktreeEntryByRoot(repoPath, rootEventId)`
- `resetProjectWorktreeRegistryStore()` registered in `resetCommunityState()`

Snapshot union gains one variant (keeping the existing three intact):

```ts
| { status: "derived"; branch: string; rootEventId: string;
    repositoryPath: string; worktreeName: string; worktreePath: string }
```

`useProjectThreadWorkspace(rootEventId, repositoryPath)` returns the observer
snapshot when it is `ready`/`error`, otherwise the derived one, otherwise
`pending`. `repositoryPath` comes from `parseProjectThreadContext(threadHead.body)`
which the panel already computes (`ProjectThreadWorkspacePanel.tsx:38-41`).

In the panel: `target` (used for the GitHub row and the destructive actions)
needs only `branch` + `repositoryPath` + `rootEventId`, so it builds from
`derived` too — that is the restart-durability win. The Workspace cell shows the
branch with detail `Restored from disk`; base-revision / behind-remote stay blank
because the registry genuinely does not know them.

The 256-entry cap stays as-is: it bounds the observer projection, it is not
harmful once the registry is the durable source, and removing it is not needed
for this outcome.

## Tests

Rust (`project_worktree_registry_parse.rs`, `#[cfg(test)]`):
- porcelain with main + managed + external + detached + prunable entries
- lowercase `buzzthreadroot` keys parse; a branch name containing `.` parses
- non-hex / short root values are rejected
- classification: `.worktrees/crew-docs-fork-identity` → `external`;
  `.buzz-worktrees/crew-02cc85801c3d` with no config row → `managed`,
  `root_event_id: None`
- PR mapping picks the open PR when a branch has both open and closed

TS (`projectWorktreeRegistryStore.test.mjs`, following
`projectThreadWorkspaceStore.test.mjs`): TTL hit/miss, epoch reset clears
entries, selector returns `null` for unknown roots.

## Validation

```bash
just desktop-tauri-test      # full crate suite, not a scoped run
just desktop-tauri-clippy    # -D warnings
just desktop-test            # desktop JS unit tests
just desktop-check           # biome + typecheck
cargo fmt --check --manifest-path desktop/src-tauri/Cargo.toml
```

Manual: run the registry against `/Users/…/Nuncio/crew` and confirm 5 managed
entries, `02cc85801c3d` with `root_event_id: None`, both `.worktrees/` checkouts
as `external`, and PR numbers from `Nuncio-hq/crew` (not `block/buzz`).

## Risk / rollback

- Registry read is strictly read-only; worst case it returns an error string and
  the UI falls back to today's observer-only behaviour.
- The `derived` variant is additive — if it misbehaves, stop returning it from
  `useProjectThreadWorkspace` and the strip is exactly as it is now.
- `gh` on a large repo can be slow; the 20 s `command_output` timeout already
  caps it, and failure degrades to git-only.
