# Brainstorm — Channel-level worktree management

Date: 2026-08-02 · Author: Claude Opus · Requester: Oscar
Scope: make worktrees visible and manageable at the **channel** level, not just
inside one thread.

## 1. What exists today (verified)

| Fact | Evidence |
| --- | --- |
| 1 project thread = 1 worktree = 1 branch | `crates/buzz-acp/src/thread_workspace.rs:68-98` |
| Naming is fully deterministic | `short = root_event_id[..12]`; branch `buzz/<short>`; path `<repo_parent>/.buzz-worktrees/<repo_name>-<short>` — `thread_workspace.rs:87-95` |
| Branch→thread mapping is persisted in git config | `branch.<branch>.buzzThreadRoot` — `thread_workspace.rs:290-296`, `434-436` |
| Thread UI = 2×3 strip + PR row | `desktop/src/features/messages/ui/ProjectThreadWorkspacePanel.tsx` |
| Lifecycle actions exist (guarded) | `close_thread_pull_request`, `delete_thread_branch`, `remove_thread_worktree` — `desktop/src-tauri/src/commands/thread_workspace.rs` |
| Worktree state store is **ephemeral** | in-memory Map, LRU cap 256, reset on community switch — `desktop/src/features/agents/projectThreadWorkspaceStore.ts:33,44` |
| Store is fed only by live agent telemetry | `thread_workspace_ready` / `_error` observer events — `crates/buzz-acp/src/pool.rs:1473-1493` |
| Strip renders only for the thread author | `MessageThreadPanel.tsx:517-523` |
| Channel timeline shows no workspace signal | `MessageThreadSummaryRow.tsx` — only "N replies · last reply …" |

### Live state of Oscar's machine (measured 2026-08-02)

```
.buzz-worktrees/crew-02cc85801c3d    59M   ← no buzzThreadRoot entry (orphan)
.buzz-worktrees/crew-0efbf738ca7b    72M
.buzz-worktrees/crew-649566de51d6    72M
.buzz-worktrees/crew-eb791333c0ee    18G   ← build artifacts, never reclaimed
                                    ───
                                     19G
```

Nothing in the product shows this. Nothing in the product can clean it.

## 2. Three real gaps

1. **Truth gap.** Workspace info survives only while the agent that created it
   is running in this session. Restart the app → the strip vanishes although the
   worktree, branch and PR all still exist.
2. **Enumeration gap.** There is no "list worktrees for this project/channel".
   Orphans (`crew-02cc85801c3d`) are invisible and accumulate; 19 GB proves it.
3. **Density gap.** The channel timeline says "3 replies". Branch, PR number, CI
   state, dirty/clean require opening each thread one by one.

## 3. Core design move: **derive, don't remember**

Everything the UI needs is already reconstructible from disk + GitHub. Replace
the ephemeral projection with one read-only Tauri command:

```
get_project_worktree_registry(repository_path) -> WorktreeRegistryEntry[]
```

Built from four cheap sources:

| Source | Gives |
| --- | --- |
| `git worktree list --porcelain` | every worktree, its path and branch (incl. orphans) |
| `git config --local --get-regexp '^branch\..*\.buzzThreadRoot$'` | branch → thread root event id |
| `git status --porcelain` + `rev-list --count base..HEAD` per worktree | dirty, ahead/behind, last commit age |
| `gh pr list --state all --json headRefName,number,state,isDraft,reviewDecision,statusCheckRollup` | **one** call for the whole repo, mapped by `headRefName` |

Then: `rootEventId` → thread message (already in the channel timeline cache) →
title, participants, last activity.

Consequences:
- The 256-entry cap and community-switch reset become irrelevant.
- Observer events downgrade from *source of truth* to *cache-invalidation signal*.
- The thread strip becomes durable across restarts for free.

Caching: React Query keyed by repository path, ~30-60 s stale time, refresh on
window focus, on `thread_workspace_ready`, and on manual click.

## 4. Surfaces (progressive disclosure, three levels)

### L1 — Channel timeline badge (cheapest, highest value)

Extend `MessageThreadSummaryRow` for project threads only:

```
👥 3 replies · ⎇ buzz/649566de · PR #42 ✓ · +214 −18
```

Rules: chips appear only when they carry information (no PR → no PR chip); no
new row, no layout shift; degrade to today's row when the registry is unknown.

### L2 — Channel header pill + Worktrees drawer

Header: `#nunciocrew-project · 4 worktrees · 2 PRs open · 19 GB`.
Click → right drawer, worktrees grouped by state:

| Bucket | Rule | Offered action |
| --- | --- | --- |
| Active | agent running, or PR open | open thread, open PR |
| Ready to merge | PR approved + checks green | open PR |
| Idle | clean, no PR, no activity > N days | remove worktree |
| Orphan | worktree with no `buzzThreadRoot`, or root not in this channel | link to thread / remove |
| Broken | directory missing, prunable | `git worktree prune` |

Bulk actions: prune broken · remove clean idle · delete merged branches. Never
touch a dirty worktree in bulk — reuse the existing refusal in
`remove_thread_worktree` (`thread_workspace.rs:75-85`).

### L3 — Thread strip (exists) + durability

Same 2×3 strip, now fed by the registry. Add disk size and "N closed PRs" in the
Workspace drawer. No visual redesign.

## 5. Decisions to confirm with Oscar

| # | Question | Recommendation |
| --- | --- | --- |
| D1 | Registry scope: channel or repo? | **Repo-scoped registry, channel-scoped view.** `git worktree list` is repo-wide; anything whose root is not a thread in this channel goes to the Orphan bucket. Prevents invisible leftovers. |
| D2 | Who sees the badges? | **Split by truth ownership.** PR / CI = GitHub truth → visible to every channel member. Worktree path / dirty / disk = machine-local truth → only the machine that owns it, labelled "on this Mac". |
| D3 | "Number of PRs" per thread | One branch normally has one open PR. Show the open one in the badge; `+N closed` only in the drawer. |
| D4 | Automatic cleanup? | **No auto-delete in v1.** Manual bulk actions with confirm. Opt-in "auto-prune merged + clean after 7 days" later, once the list has proven trustworthy. |
| D5 | Disk size in the list? | Yes — it is the reason this feature exists (18 GB in one worktree). Compute lazily/async per row, not in the blocking registry read. |

## 6. Risks

- **`gh` picks the wrong repo.** In `crew`, `origin=Nuncio-hq/crew` and
  `upstream=block/buzz` with **no `gh` default set**; `gh pr list` today resolves
  to `block/buzz` (returns PR #4249 etc.). Existing per-thread PR lookup in
  `desktop/src-tauri/src/commands/thread_github.rs:110` inherits this. Fix: pin
  `--repo` from `git remote get-url origin`. **This is a live bug, independent of
  this feature.**
- **`gh` missing or unauthenticated** → degrade to git-only registry, hide PR chips.
- **Cost on wide repos.** `git status` × 20 worktrees is slow; run concurrently,
  cap, and make per-row detail lazy.
- **Destructive actions.** Keep the existing confirm-dialog + refuse-if-dirty
  pattern; bulk paths must reuse the same guards, not bypass them.

## 7. Suggested phasing

| Phase | Delivers | Why first |
| --- | --- | --- |
| 1 | `get_project_worktree_registry` + thread strip reads it | Fixes the truth gap with zero new UI; unblocks everything else |
| 2 | L1 channel timeline badges | Highest value per line of code |
| 3 | L2 header pill + worktrees drawer + bulk management | Solves the 19 GB problem |
| 0.5 | Pin `gh --repo` from origin | Small, independent, currently wrong |

## Unresolved questions

- D1–D5 above.
- Should the drawer live in the channel, or in the existing Projects screen
  (`desktop/src/features/projects/ui/`)? Channel-first matches the ask; the
  Projects screen would serve a cross-channel view later.
