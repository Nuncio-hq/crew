# GitHub CLI resolution + PR status visibility — plan index

Date: 2026-08-04 · Owner: Oscar · Author: Claude Opus
Trigger: Oscar reported worktree/PR status missing from thread info in NuncioCrew v0.0.7.
Status: **Phases 1–3 merged** (#40, #42) · Packaged Finder verify on v0.0.8 still pending

## Outcome

PR and CI status render in the packaged NuncioCrew app, and when they genuinely
cannot be fetched the UI says so instead of rendering nothing.

Acceptance criteria:

1. Launching `NuncioCrew.app` from Finder on a Mac where `gh` is installed only
   under `/opt/homebrew/bin` shows PR/CI state on a project thread that has an
   open PR.
2. The channel header pill shows `N worktrees · M PRs open` in that same launch,
   not just `N worktrees`.
3. When `gh` is missing or its call fails, the thread bar and the worktrees
   drawer show a degraded affordance naming the cause — distinguishable from
   "this branch has no PR".
4. `just ci` green.

## Non-goals

- No change to how PRs are discovered (still `gh pr list --head <branch>`).
- No bundling or auto-install of `gh`.
- No relay-published PR state — everything stays derived on the viewer's machine.
- No redesign of the sticky bar's visual language.

## Root cause (verified 2026-08-04 at `14dc86bb1`)

The feature **is** in v0.0.7. `crew-v0.0.7` = `14dc86bb1` = `origin/main` HEAD;
PRs #8, #28, #29 are all ancestors. Release build verifies the checkout matches
`inputs.ref` (`nuncio-crew-release.yml:65`).

What breaks is binary resolution. Five call sites spawn `gh` by bare name, none
of them setting `PATH`:

| File | Line | Effect when `gh` is unresolvable |
| --- | --- | --- |
| `desktop/src-tauri/src/commands/thread_github.rs` | 118 | `find_pull_request_number` → `None` → `unavailable()` |
| `desktop/src-tauri/src/commands/thread_github.rs` | 141 | `read_pull_request` → `None` → `unavailable()` |
| `desktop/src-tauri/src/commands/project_worktree_registry_github.rs` | 60 | `fetch_pull_requests_by_branch` → `None` → registry `github: unavailable` |
| `desktop/src-tauri/src/commands/thread_workspace.rs` | 157 | `close_thread_pull_request` list step errors |
| `desktop/src-tauri/src/commands/thread_workspace.rs` | 175 | `close_thread_pull_request` close step errors |

macOS GUI apps inherit launchd's PATH, not the shell's. Measured on Oscar's Mac:

```
$ launchctl getenv PATH                                   → (empty ⇒ /usr/bin:/bin:/usr/sbin:/sbin)
$ which gh                                                → /opt/homebrew/bin/gh
$ env -i PATH=/usr/bin:/bin:/usr/sbin:/sbin sh -c 'command -v gh'
gh: NOT FOUND
```

So in the packaged app every `gh` call fails, always. Running via `just dev`
from a terminal inherits the shell PATH and works — which is why this never
showed up in development.

`git` is unaffected: `/usr/bin/git` exists on the default PATH.

### Why it looks like the feature was never shipped

Nothing renders the degraded state. `availability` is plumbed end-to-end
(`thread-workspace-types.ts:51` and `:85`) but **no component reads it** —
verified by grep across `desktop/src`. The chips are gated purely on
`pullRequest` being non-null (`ProjectThreadWorkspacePanel.tsx:287`), and the
GitHub row likewise (`:385`). So "gh is broken" and "this branch has no PR"
both render as empty space.

The registry path is the same: `ChannelWorktreesPill.tsx:23-27` builds its label
from `countOpenPullRequests`, which is 0 when the fetch failed — the pill just
silently drops the `· M PRs open` half.

## Decisions

### D1 — Resolve `gh` through the existing discovery helper, do not hand-roll a PATH list ✅

`crate::managed_agents::find_command` (`managed_agents/discovery.rs:999`) already
does managed-bin → env PATH → login shell → `/opt/homebrew/bin`, `/usr/local/bin`,
`/usr/bin`, linuxbrew → nvm. Reachable from `commands/` via `pub use discovery::*`
(`managed_agents/mod.rs:50`). Reusing it keeps one resolution policy in the app.

Caveat found while scouting: `find_command` currently has **no callers outside
`discovery.rs`** (grepped `desktop/src-tauri/src`). Phase 1 must confirm it
compiles from `commands/` rather than assume it.

### D2 — Distinguish "gh missing" from "gh failed" from "no PR" ✅

Today `unavailable` collapses "binary not found", "not logged in", "no remote",
and "network down" into one silent bucket. The UI cannot help the user without
knowing which. Widen the enum rather than adding a free-text field.

### D3 — Expanding the thread bar outside focus mode ⚠️ needs Oscar's call

Worktree detail (branch name, commits-behind) only exists in the expanded grid,
which needs `isFocusMode && expanded` (`ProjectThreadWorkspacePanel.tsx:169`),
and the chevron that sets `expanded` only renders when `isFocusMode` (`:318`).
In the normal right-hand thread panel there is **no way to reach it** — you get
the chip row, and "Workspace" opens a drawer.

That may be deliberate: the grid is `grid-cols-3` and the side panel is narrow.
Phase 3 proposes allowing expand everywhere with a responsive grid, but it is a
design change, so it ships separately and last. See
[`phase-03-thread-panel-expand.md`](phase-03-thread-panel-expand.md).

## Phases

| # | Phase | Depends on | Ships |
| --- | --- | --- | --- |
| 1 | [`gh` binary resolution](phase-01-gh-binary-resolution.md) | — | Fixes the bug. PR/CI status appears in the packaged app. |
| 2 | [Degraded-state affordance](phase-02-degraded-affordance.md) | 1 | Makes a future breakage self-diagnosing instead of invisible. |
| 3 | [Thread panel expand](phase-03-thread-panel-expand.md) | — | Reaches worktree detail without focus mode. Gated on D3. |

Phase 1 alone satisfies acceptance criteria 1 and 2, and is worth shipping on
its own if 2 and 3 slip.

## Risks

| Risk | Mitigation |
| --- | --- |
| `find_command` spawns a login shell → blocks the tokio runtime | Call it inside `tokio::task::spawn_blocking`. `login_shell_path` caches its result (`discovery.rs:836`), so only the first resolution can be slow. |
| Caching a `None` means installing `gh` later needs an app restart | Cache only successful resolutions; a miss re-resolves. Login-shell PATH is already cached, so a repeated miss is cheap. |
| Widening the availability enum breaks the TS union | Blast radius is small — 3 sites: `thread-workspace-types.ts:51`, `:85`, and `projectThreadGitHubStore.ts:73`. Verified by grep for `"unavailable"` across `desktop/src`. |
| `gh` shells out to `git` internally | Pass the augmented PATH via `readiness::cli_probe::augmented_path()` when spawning, not just the resolved binary path. |
| Windows/Linux regressions | `find_command` already handles both; no platform-specific code is added. |

## Verification

Repro without repackaging — reproduces the GUI PATH from a terminal:

```bash
env -i HOME="$HOME" PATH=/usr/bin:/bin:/usr/sbin:/sbin \
  /Applications/NuncioCrew.app/Contents/MacOS/NuncioCrew
```

Before phase 1 the PR chips are absent; after, they render. The full packaged
launch-from-Finder check still runs once before release.

## Unresolved questions

1. **D3** — allow expanding the thread bar outside focus mode, or keep the
   drawer as the only path to worktree detail in the side panel?
2. Should a `gh`-missing state link to install instructions, or just name the
   cause? Phase 2 assumes the latter (KISS).
3. Oscar's screenshot could not be fetched (media endpoint returns
   `authentication failed` to an unauthenticated request), so the exact surface
   in it is still unconfirmed. The plan covers both the thread bar and the
   channel pill, so it should hold either way.
