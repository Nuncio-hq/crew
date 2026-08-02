---
title: "Scout: Phase 1 channel worktree registry patterns"
tags: [scout, channel-worktree, phase-01]
status: active
created: 2026-08-02
---

# Scout report — Phase 1 worktree registry

Work context: `/Users/a1241968/Desktop/Oscar/LilGroup/Nuncio/.buzz-worktrees/crew-649566de51d6`

Copy these patterns for `get_project_worktree_registry` + `projectWorktreeRegistryStore`.

---

## Path corrections vs phase-01 plan

| Plan said | Actual |
| --- | --- |
| `desktop/src/features/agents/projectThreadGitHubStore.ts` | **`desktop/src/features/messages/lib/projectThreadGitHubStore.ts`** |
| GitHub store TTL 60s mirror | GitHub store TTL is **30_000 ms**; plan wants registry TTL **60 s** (intentional difference) |
| `thread_github_target` near `pub use` | Module is `mod` only — **not** `pub use` (internal helper) |

---

## 1. `thread_workspace_git.rs` — git runners

**Path:** `desktop/src-tauri/src/commands/thread_workspace_git.rs` (185 lines)

**Signatures:**

```rust
pub(crate) async fn git_output_at<I, S>(cwd: &Path, args: I) -> Result<String, String>
pub(crate) async fn git_output_dir<I, S>(git_dir: &Path, args: I) -> Result<String, String>
pub(crate) async fn git_success_at / git_success_dir -> Result<bool, String>
pub(crate) async fn command_output(command: &mut Command) -> Result<std::process::Output, String>
pub(crate) async fn validate_target(...) -> Result<ThreadWorkspaceTarget, String>
```

**How commands run:**

- `tokio::process::Command`
- `git -C <cwd>` vs `git --git-dir <dir>`
- `COMMAND_TIMEOUT = 20s`, `kill_on_drop(true)`
- Failures → `Result<_, String>` (stderr trim, or `"Command failed."` / `"Command timed out."`)

```94:112:desktop/src-tauri/src/commands/thread_workspace_git.rs
pub(crate) async fn git_output_at<I, S>(cwd: &Path, args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new("git");
    command.arg("-C").arg(cwd).args(args);
    output_text(command_output(&mut command).await?)
}

pub(crate) async fn git_output_dir<I, S>(git_dir: &Path, args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new("git");
    command.arg("--git-dir").arg(git_dir).args(args);
    output_text(command_output(&mut command).await?)
}
```

```161:175:desktop/src-tauri/src/commands/thread_workspace_git.rs
pub(crate) async fn command_output(command: &mut Command) -> Result<std::process::Output, String> {
    command.kill_on_drop(true);
    let output = tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| "Command timed out.".to_string())?
        .map_err(|error| format!("Could not start command: {error}"))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if message.is_empty() {
            "Command failed.".to_string()
        } else {
            message
        });
    }
    Ok(output)
}
```

**Reuse for registry:** canonicalize via `git_output_at(path, ["rev-parse", "--show-toplevel"])` + `--git-common-dir`; porcelain via `git_output_dir(&common, ["worktree", "list", "--porcelain"])`.

---

## 2. `thread_github.rs` — types, PR list, parse_check

**Path:** `desktop/src-tauri/src/commands/thread_github.rs` (206 lines)

**Types / serde:**

| Item | Serde |
| --- | --- |
| Structs (`ThreadGitHubStatus`, `ThreadPullRequest`, …) | `#[serde(rename_all = "camelCase")]` |
| Enums (`ThreadGitHubAvailability`) | `#[serde(rename_all = "kebab-case")]` → `"available"` / `"unavailable"` |
| Command | `Result<T, String>` |

**Command:**

```rust
#[tauri::command]
pub async fn get_thread_github_status(
    repository_path: String,
    branch: String,
    root_event_id: String,
) -> Result<ThreadGitHubStatus, String>
```

**PR list pattern** (`gh` + optional `--repo` + `command_output`):

```113:133:desktop/src-tauri/src/commands/thread_github.rs
async fn find_pull_request_number(
    repository: &std::path::Path,
    repo: Option<&str>,
    branch: &str,
) -> Option<u64> {
    let mut command = Command::new("gh");
    command
        .args(["pr", "list", "--state", "all", "--head", branch])
        .args(["--json", "number", "--limit", "1"])
        .current_dir(repository);
    if let Some(repo) = repo {
        command.args(["--repo", repo]);
    }
    let output = command_output(&mut command).await.ok()?;
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).ok()?;
    Some(
        rows.first()
            .and_then(|row| row["number"].as_u64())
            .unwrap_or(0),
    )
}
```

**`parse_check` — currently private; plan says extract to `pub(crate)`:**

```152:170:desktop/src-tauri/src/commands/thread_github.rs
fn parse_check(value: &serde_json::Value) -> Option<ThreadPullRequestCheck> {
    let name = value["name"]
        .as_str()
        .or_else(|| value["context"].as_str())?
        .to_string();
    let state = ["conclusion", "state", "status"]
        .iter()
        .find_map(|key| value[*key].as_str().filter(|state| !state.is_empty()))?
        .to_string();
    Some(ThreadPullRequestCheck {
        name,
        state,
        url: value["detailsUrl"]
            .as_str()
            .or_else(|| value["targetUrl"].as_str())
            .map(str::to_string),
        workflow: value["workflowName"].as_str().map(str::to_string),
    })
}
```

**Unit tests in-file:** `#[cfg(test)] mod tests` — `parses_check_runs_and_commit_statuses` (lines 179–206).

---

## 3. `thread_github_target.rs` — origin_repo_target

**Path:** `desktop/src-tauri/src/commands/thread_github_target.rs` (111 lines)

```rust
pub(crate) async fn origin_repo_target(repository_path: &Path) -> Option<String>
fn repo_target_from_remote_url(url: &str) -> Option<String>  // private
```

- github.com → `owner/name`
- enterprise → `host/owner/name`
- `None` → caller omits `--repo` (gh default)

```15:20:desktop/src-tauri/src/commands/thread_github_target.rs
pub(crate) async fn origin_repo_target(repository_path: &Path) -> Option<String> {
    let url = git_output_at(repository_path, ["remote", "get-url", "origin"])
        .await
        .ok()?;
    repo_target_from_remote_url(url.trim())
}
```

**Tests in-file:** URL forms, enterprise host keep, reject bad URLs (lines 57–111).  
**Not** `pub use` in `mod.rs` — import via `super::thread_github_target::origin_repo_target`.

---

## 4. `mod.rs` — module registration

**Path:** `desktop/src-tauri/src/commands/mod.rs` (123 lines)

Near thread modules:

```58:63:desktop/src-tauri/src/commands/mod.rs
mod thread_github;
mod thread_github_target;
mod thread_workspace;
mod thread_workspace_git;
#[cfg(test)]
mod thread_workspace_tests;
```

`pub use` (helpers stay private):

```116:117:desktop/src-tauri/src/commands/mod.rs
pub use thread_github::*;
pub use thread_workspace::*;
```

**Phase 1 add:**

```rust
mod project_worktree_registry;
mod project_worktree_registry_parse;
mod project_worktree_registry_github;
// ...
pub use project_worktree_registry::*;
```

(`parse` / `github` siblings: `mod` only, like `thread_github_target` / `thread_workspace_git`.)

---

## 5. `lib.rs` — invoke_handler

**Path:** `desktop/src-tauri/src/lib.rs`  
Handler starts ~623; thread commands ~664–668:

```664:668:desktop/src-tauri/src/lib.rs
            remove_thread_worktree,
            delete_thread_branch,
            close_thread_pull_request,
            get_thread_workspace_lifecycle,
            get_thread_github_status,
```

**Insert:** `get_project_worktree_registry` next to those (after `get_thread_github_status` is natural).

---

## 6. GitHub FE store — cache + sync + TTL + reset

**Path:** `desktop/src/features/messages/lib/projectThreadGitHubStore.ts` (97 lines)  
**Not** under `features/agents/`.

**Pattern to copy for `projectWorktreeRegistryStore.ts`:**

```ts
// Map + listeners + cacheEpoch
// CACHE_TTL_MS = 30_000 (registry plan: 60_000)
// load(target, force): skip if fresh; coalesce promise; ignore stale epoch
// useSyncExternalStore(subscribe, getSnapshot)
// reset*: cacheEpoch += 1; clear; notify
```

```12:27:desktop/src/features/messages/lib/projectThreadGitHubStore.ts
export type ProjectThreadGitHubSnapshot =
  | { status: "pending" }
  | { status: "ready"; value: ThreadGitHubStatus };

const PENDING: ProjectThreadGitHubSnapshot = { status: "pending" };
const CACHE_TTL_MS = 30_000;
const entries = new Map<
  string,
  {
    expiresAt: number;
    promise?: Promise<void>;
    snapshot: ProjectThreadGitHubSnapshot;
  }
>();
const listeners = new Set<() => void>();
let cacheEpoch = 0;
```

```76:97:desktop/src/features/messages/lib/projectThreadGitHubStore.ts
export function useProjectThreadGitHub(target: Target | null) {
  const key = target ? cacheKey(target) : null;
  const getSnapshot = React.useCallback(
    () => (key ? (entries.get(key)?.snapshot ?? PENDING) : PENDING),
    [key],
  );
  const snapshot = React.useSyncExternalStore(subscribe, getSnapshot);
  React.useEffect(() => {
    if (target) void load(target, false);
  }, [target]);
  const refresh = React.useCallback(
    () => (target ? load(target, true) : Promise.resolve()),
    [target],
  );
  return { refresh, snapshot };
}

export function resetProjectThreadGitHubStore(): void {
  cacheEpoch += 1;
  entries.clear();
  notify();
}
```

**Gotcha:** errors collapse to `ready` + `availability: "unavailable"` (never leave pending stuck). Registry should similarly degrade `github: unavailable`, not throw.

**No dedicated `*GitHubStore*.test.mjs`** — mirror `projectThreadWorkspaceStore.test.mjs` instead.

---

## 7. `projectThreadWorkspaceStore.ts` — observer projection

**Path:** `desktop/src/features/agents/projectThreadWorkspaceStore.ts` (215 lines)

**Snapshot union (today — no `derived` yet):**

```ts
export type ProjectThreadWorkspaceSnapshot =
  | { status: "pending" }
  | { status: "ready"; agentPubkey; baseSource; baseRevision; branch;
      conversationId; rootEventId; repositoryPath; remoteDefaultBranch;
      commitsBehindRemote; worktreeName; worktreePath }
  | { status: "error"; agentPubkey; conversationId; message; rootEventId };
```

**Mechanics:**

- Fed only by `ingestProjectThreadWorkspaceEvent` (`thread_workspace_ready` / `_error`)
- Map keyed by `rootEventId`, cap **256** LRU
- Watermark: `timestampMs` → `seq` → `agentPubkey` lexicographic
- `getProjectThreadWorkspaceSnapshot` reinserts for LRU touch
- Community save/restore via `savedByCommunity` (separate from GitHub TTL cache)
- `resetProjectThreadWorkspaceStore()` clears active map only (used on community switch after save)

**Phase 1:** add `| { status: "derived"; branch; rootEventId; repositoryPath; worktreeName; worktreePath }` — keep pending/ready/error intact.

---

## 8. `useProjectThreadWorkspace.ts` — current hook

**Path:** `desktop/src/features/agents/useProjectThreadWorkspace.ts` (14 lines)

```ts
export function useProjectThreadWorkspace(
  rootEventId: string | null | undefined,
) {
  const getSnapshot = React.useCallback(
    () => getProjectThreadWorkspaceSnapshot(rootEventId),
    [rootEventId],
  );
  return React.useSyncExternalStore(subscribeAgentObserverStore, getSnapshot);
}
```

**Phase 1 change:** accept `repositoryPath`; return observer `ready`/`error`, else registry-derived, else `pending`. Must also subscribe to registry store (or combine notifies) — today only `subscribeAgentObserverStore`.

---

## 9. API types + `agentControl.ts`

**Types:** `desktop/src/shared/api/thread-workspace-types.ts` (53 lines)  
Add registry types alongside `ThreadGitHubStatus` / lifecycle types.

**API:** `desktop/src/shared/api/agentControl.ts`

```47:81:desktop/src/shared/api/agentControl.ts
type ThreadWorkspaceTarget = {
  repositoryPath: string;
  branch: string;
  rootEventId: string;
};

export function getThreadWorkspaceLifecycle(
  input: ThreadWorkspaceTarget & { worktreePath: string },
): Promise<ThreadWorkspaceLifecycle> {
  return invokeTauri("get_thread_workspace_lifecycle", input);
}

export function getThreadGitHubStatus(
  input: ThreadWorkspaceTarget,
): Promise<ThreadGitHubStatus> {
  return invokeTauri("get_thread_github_status", input);
}
```

**Pattern for new API:**

```ts
export function getProjectWorktreeRegistry(
  input: { repositoryPath: string },
): Promise<ProjectWorktreeRegistry> {
  return invokeTauri("get_project_worktree_registry", input);
}
```

Tauri camelCases **top-level args**; Rust structs need `#[serde(rename_all = "camelCase")]` on response fields. Enums in this area use **kebab-case** (`available` / `unavailable`).

---

## 10. `ProjectThreadWorkspacePanel.tsx` — snapshot use

**Path:** `desktop/src/features/messages/ui/ProjectThreadWorkspacePanel.tsx` (168 lines)

```38:69:desktop/src/features/messages/ui/ProjectThreadWorkspacePanel.tsx
  const context = React.useMemo(
    () => parseProjectThreadContext(threadHead.body),
    [threadHead.body],
  );
  const workspace = useProjectThreadWorkspace(threadHead.id);
  // ...
  const target = React.useMemo(
    () =>
      workspace.status === "ready" && workspace.repositoryPath
        ? {
            branch: workspace.branch,
            repositoryPath: workspace.repositoryPath,
            rootEventId: workspace.rootEventId,
          }
        : null,
    [workspace],
  );
  const { refresh: refreshGitHub, snapshot: githubSnapshot } =
    useProjectThreadGitHub(target);
```

Workspace cell detail (lines 117–127): only `ready` / `error` / else `"Preparing"`.

**Phase 1:**

- Pass `context`-derived repo path into hook
- Build `target` from `ready` **or** `derived`
- Workspace cell: `derived` → branch title + detail `"Restored from disk"`

---

## 11. `useCommunityInit.ts` — resetCommunityState

**Path:** `desktop/src/features/communities/useCommunityInit.ts`

```53:78:desktop/src/features/communities/useCommunityInit.ts
function resetCommunityState({
  resetAvatarState,
}: {
  resetAvatarState: boolean;
}): void {
  // ...
  resetProjectThreadGitHubStore();
  clearSearchHitEventCache();
  clearMarkdownNodeCache();
}
```

**Mandatory:** import + call `resetProjectWorktreeRegistryStore()` next to GitHub reset (line ~75).

Note: workspace store uses save/restore around community switch, **not** this reset list — registry is TTL cache like GitHub → reset here.

---

## 12. Existing tests

| File | What |
| --- | --- |
| `desktop/src-tauri/src/commands/thread_github.rs` `#[cfg(test)]` | `parse_check` only |
| `desktop/src-tauri/src/commands/thread_github_target.rs` `#[cfg(test)]` | remote URL parsing |
| `desktop/src-tauri/src/commands/thread_workspace_tests.rs` (146) | integration: remove worktree / delete branch fixtures |
| `desktop/src/features/agents/projectThreadWorkspaceStore.test.mjs` (255) | pending/ready/error, watermark, community round-trip, LRU cap |
| `desktop/src/features/messages/lib/projectThreadWorkspace.test.mjs` (104) | context/steps helpers — **not** store |
| *(missing)* | `projectThreadGitHubStore` tests |

**New tests (per plan):**

- Rust: `project_worktree_registry_parse.rs` `#[cfg(test)]`
- TS: `projectWorktreeRegistryStore.test.mjs` (follow workspace store test style: `node:test` + `.ts` import)

---

## Neighboring command file sizes / conventions

| File | Lines |
| --- | --- |
| `thread_github_target.rs` | 111 |
| `thread_workspace_git.rs` | 185 |
| `thread_workspace.rs` | 205 |
| `thread_github.rs` | 206 |
| `thread_workspace_tests.rs` | 146 |
| `project_git_push.rs` | 136 |
| `project_git*.rs` siblings | 136–988 (split by concern) |

**Conventions to match:**

- Split registry into 3 files (~100–200 lines each) — fits neighborhood
- `#[serde(rename_all = "camelCase")]` on structs; **kebab-case** on availability/kind enums
- All Tauri commands: `Result<T, String>` with human sentences ending in `.`
- `pub(crate)` for shared helpers; only command module `pub use`d
- Prefer `command_output` / `git_output_*` over raw `Command` without timeout

**Path formula (ACP, for classification):**  
`crates/buzz-acp/src/thread_workspace.rs` ~91–96:

```
parent = repo_root.parent()/.buzz-worktrees
worktree = parent/{repo_name}-{short_root}
branch = buzz/{short_root}
```

---

## Gotchas

1. **`parse_check` is private** — extract/`pub(crate)` before registry GitHub module imports it; update in-file tests accordingly.
2. **`thread_github_target` is not re-exported** — use `super::thread_github_target::…`.
3. **GitHub store path** is under `messages/lib/`, not `agents/` — put new registry store under `agents/` per plan (alongside workspace store) is fine; reset import path must be correct.
4. **Observer vs TTL stores differ:** workspace = event projection + community save/restore; GitHub/registry = Map+epoch+TTL+`resetCommunityState`.
5. **Hook subscription gap:** adding registry fallback requires notifying React when registry loads; `subscribeAgentObserverStore` alone will not re-render on registry ready.
6. **`target` today requires `ready` + `repositoryPath`** — restart durability needs `derived` in that `useMemo`.
7. **Config key casing:** git emits lowercase `buzzthreadroot`; parse case-insensitively; keep branch middle segment (may contain `.`).
8. **`gh` failure must not fail command** — unlike `get_thread_github_status` which returns `Unavailable` status object; registry returns `github: unavailable` + empty PR vecs while still returning git entries.
9. **File size:** keep new command files under ~200 lines (split already planned).
10. **No React Query** — copy GitHub store module pattern.

---

## Unresolved questions

1. Should `parse_check` move to a shared module (e.g. `thread_github_checks.rs`) or stay in `thread_github.rs` as `pub(crate)`?
2. Should registry store live in `features/agents/` (plan) or `features/messages/lib/` (next to GitHub store)?
3. For hook dual-subscribe: merge listeners manually, or have registry notify also tick observer store? (No existing dual-store hook found.)
)
