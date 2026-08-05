# Phase 3 — Channel header pill + Worktrees drawer + cleanup

Depends on: Phase 1.
Delivers: the surface that solves the 19 GB problem — enumerate, inspect, and
remove worktrees without a terminal.

## Surface

Header (`desktop/src/features/channels/ui/ChannelScreenHeader.tsx`, next to
`ChannelHeaderStatusBadge` at line 154): a pill

```
5 worktrees · 2 PRs open
```

Disk total is **not** in the pill — it would force a `du` on every render. It
appears in the drawer header once sizes have been fetched
(`19.1 GB across 5 worktrees`).

Click → right drawer, grouped:

| Bucket | Rule | Actions |
| --- | --- | --- |
| Active | agent running for the thread, or PR open | Open thread · Open PR |
| Ready to merge | PR open, `reviewDecision APPROVED`, checks passing | Open PR |
| Idle | managed, clean, no open PR, no commit in 7 days | Remove worktree |
| Orphan | managed, `rootEventId` missing **or** not a thread root of this channel | Open thread (if resolvable) · Remove worktree |
| Broken | `prunable` — directory gone | Prune |
| Other checkouts | `kind: external` / `main` | none — read-only, collapsed |

`crew-02cc85801c3d` lands in Orphan; the two `.worktrees/` human checkouts land
in "Other checkouts" and are never selectable for bulk actions (D1).

## The blocker to design around

`validate_target` (`thread_workspace_git.rs:16-56`) demands **both** a
`branch.<b>.buzzThreadRoot` config row **and** the claim file
`buzz-thread-workspace-roots/<short>.root`, and it derives the expected branch
from a supplied 64-hex root id. An orphan has none of that, so
`remove_thread_worktree` **cannot** clean up exactly the case this feature
exists for. That guard is correct for its own job (it authorizes a destructive
action against a claimed thread identity) and must not be loosened.

Phase 3 therefore adds a second, path-authorized command with its own guards.

## New commands

`desktop/src-tauri/src/commands/project_worktree_cleanup.rs` (new):

```rust
#[tauri::command]
pub async fn remove_project_worktree(
    repository_path: String,
    worktree_path: String,
) -> Result<ThreadWorkspaceActionResult, String>;

#[tauri::command]
pub async fn prune_project_worktrees(
    repository_path: String,
) -> Result<ThreadWorkspaceActionResult, String>;
```

`remove_project_worktree` refuses unless **all** hold:

1. `repository_path` canonicalizes to a git repo; take its `--git-common-dir`.
2. `worktree_path` canonicalizes and appears in that repo's
   `git worktree list --porcelain`.
3. It is not the main worktree.
4. Its canonical parent is exactly `<repo_root_parent>/.buzz-worktrees`.
5. Its checked-out branch matches `^buzz/[0-9a-f]{12}$`.
6. `git -C <worktree> status --porcelain` is empty — otherwise return the same
   `refused` message as `thread_workspace.rs:88-90`.

Guards 4 and 5 are what keep `.worktrees/crew-docs-fork-identity` and any other
human checkout unreachable, independent of what the UI sends.

Branch deletion stays a separate, explicit action and keeps using
`delete_thread_branch` for identity-verified threads. For orphan branches, v1
leaves the branch in place and says so in the row ("branch kept") — deleting a
branch whose provenance cannot be verified is not worth the risk here.

`get_project_worktree_details(repository_path, worktree_path)` (same module or a
sibling) returns the per-row lazy data (D5): `dirty`, `ahead`, `behind`,
`last_commit_at`, `disk_bytes`. Called only when a row is expanded, one at a
time, with the result cached in the store until the next registry refresh.
`disk_bytes` via `du -sk` on the canonical worktree path, behind the existing
20 s timeout — an 18 GB tree is the reason this is not in the registry read.

## Bulk actions

Only two, both explicit:

- **Prune broken** — one `git worktree prune`.
- **Remove selected idle** — iterates `remove_project_worktree` per path. It
  never bypasses the guards; a dirty worktree that turned dirty since the last
  refresh simply comes back `refused` and stays in the list with the reason.

Confirm dialog lists every path and the total size to be freed, and reports a
per-path result summary afterwards (`3 removed · 1 refused (uncommitted changes)`).

## Files

| File | Change |
| --- | --- |
| `desktop/src-tauri/src/commands/project_worktree_cleanup.rs` | **new** — the two commands + guards |
| `desktop/src-tauri/src/commands/project_worktree_details.rs` | **new** — lazy per-row detail incl. `du` |
| `desktop/src-tauri/src/commands/mod.rs`, `lib.rs` | register + expose |
| `desktop/src/shared/api/agentControl.ts`, `thread-workspace-types.ts` | bindings + types |
| `desktop/src/features/channels/ui/ChannelWorktreesPill.tsx` | **new** |
| `desktop/src/features/channels/ui/ChannelWorktreesDrawer.tsx` | **new** — sheet shell + buckets |
| `desktop/src/features/channels/ui/ChannelWorktreeRow.tsx` | **new** — one row + expand + actions |
| `desktop/src/features/channels/lib/worktreeBuckets.ts` | **new** — pure bucketing, unit-tested |
| `desktop/src/features/channels/ui/ChannelScreenHeader.tsx` | mount the pill |

Follow the existing sheet pattern (`ChannelManagementSheet.tsx`) rather than a
new drawer primitive. Keep every file ≲200 lines — split rows/buckets as above.

## Channel scoping input

Bucketing needs the set of thread root ids in this channel to tell Orphan from
Active. The channel timeline already holds top-level messages; pass
`Set<string>` of root ids down from `ChannelScreen`. An entry whose
`rootEventId` is set but absent from that set is still Orphan **for this
channel** — label it "from another channel" rather than "unknown" so the copy is
not misleading.

## Tests

Rust:
- guard matrix for `remove_project_worktree`: path outside `.buzz-worktrees`,
  non-`buzz/` branch, main worktree, unlisted path, dirty tree → each refused or
  errored, and **no** `git worktree remove` is invoked
- prune on a repo with no broken worktrees is a no-op success

TS (`worktreeBuckets.test.mjs`):
- orphan (no root) vs orphan (root from another channel) vs active
- `external`/`main` never appear in an actionable bucket
- idle threshold boundary at exactly 7 days
- ready-to-merge requires both approval and passing checks

## Validation

```bash
just desktop-tauri-test
just desktop-tauri-clippy
just desktop-test
just desktop-check
```

Manual, on the real repo: open the drawer, confirm 5 managed worktrees + the
`.worktrees/` pair listed read-only, expand `crew-eb791333c0ee` and confirm the
size reads ~18 GB, then remove one clean idle worktree and confirm
`git worktree list` drops it and the disk is freed.

Screenshots for the PR via `just desktop-screenshot` + `scripts/post-screenshots.sh`
(never `buzz upload`), with distinct-hash verification before posting.

## Risk / rollback

- **Destructive surface** — this is the phase that earns full review. The guards
  above are the whole safety argument; they are enforced in Rust so a UI bug
  cannot widen them.
- `du` on a huge worktree is the main latency risk: on demand only, one row at a
  time, timeout-capped.
- Rollback: unmount the pill. The commands are additive and nothing else calls
  them.
