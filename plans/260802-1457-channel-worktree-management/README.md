# Channel-level worktree management — plan index

Date: 2026-08-02 · Owner: Oscar · Author: Claude Opus
Source brainstorm: [`plans/reports/brainstorm-260802-1417-channel-worktree-management.md`](../reports/brainstorm-260802-1417-channel-worktree-management.md)
Status: **planned, not started** (Phase 0.5 shipped)

## Outcome

Make thread worktrees visible and manageable from the channel, not only from
inside a running agent session. Concretely:

1. Workspace/PR info on a project thread survives an app restart.
2. A channel can enumerate every worktree of its project repo — including
   orphans nobody remembers creating.
3. Idle and broken worktrees can be removed from the UI without a terminal.

## Non-goals (v1)

- No auto-deletion of anything (D4).
- No relay-published worktree/PR state — every chip is derived on the viewer's
  own machine (D2). Cross-member visibility is a v2 idea, sketched below.
- No redesign of the existing thread strip; only its data source changes.
- No touching human-created worktrees outside `.buzz-worktrees/`.

## Measured state (this Mac, 2026-08-02)

`git worktree list --porcelain` in `/Users/a1241968/Desktop/Oscar/LilGroup/Nuncio/crew`:

| Class | Count | Detail |
| --- | --- | --- |
| main | 1 | `crew` on `main` |
| managed (`.buzz-worktrees/crew-<12hex>`, branch `buzz/<12hex>`) | 5 | `02cc85801c3d`, `0efbf738ca7b`, `649566de51d6`, `cd2e8ed73d4b`, `eb791333c0ee` |
| human (`.worktrees/…`, arbitrary branches) | ≥2 | `crew-docs-fork-identity`, `crew-managed-node-tools-arch-scoping` |

`git config --local --get-regexp '…buzzThreadRoot$'` returns 4 entries → the
managed worktree `crew-02cc85801c3d` has **no** thread-root record (orphan).
Measured disk earlier today: ~19 GB total, 18 GB in `crew-eb791333c0ee` alone.

The human worktrees under `.worktrees/` are the reason the registry needs an
explicit *managed vs external* classification: bulk actions must never see them.

## Decisions D1–D5 (locked)

Answers adopted from the brainstorm recommendations. **D2 is revised** — the
original recommendation rested on a premise that turned out to be false.

### D1 — Registry is repo-scoped; the view is channel-scoped. ✅ as recommended

`git worktree list` is repo-wide, so the registry reads a repository and returns
every worktree with a `kind`:

| kind | Rule | Actionable |
| --- | --- | --- |
| `main` | the primary worktree | never |
| `managed` | canonical parent == `<repo_parent>/.buzz-worktrees` **and** branch matches `^buzz/[0-9a-f]{12}$` | yes |
| `external` | anything else (e.g. `.worktrees/crew-docs-fork-identity`) | never — listed read-only |

A channel view then buckets `managed` entries by whether their `rootEventId`
matches a thread root in that channel; unmatched ones are Orphan. Nothing is
invisible, and nothing outside the Buzz namespace is ever a delete candidate.

### D2 — v1 is local-derived only. ⚠️ revised

Original recommendation: "PR/CI = GitHub truth → visible to every channel
member." Two facts found while planning contradict the premise:

1. The repo path in a project thread comes from the thread-root message
   (`parseProjectThreadContext` → `path=/Users/a1241968/…/crew`,
   `desktop/src/features/messages/lib/projectThreadWorkspace.ts:33-50`). That
   path is the *authoring machine's* path; on another member's Mac it does not
   resolve.
2. The thread strip has **no** author gate (`MessageThreadPanel.tsx:609` renders
   `ProjectThreadWorkspacePanel` unconditionally; the component self-nulls). The
   brainstorm's claim that it renders only for the thread author was wrong. It
   appears author-only today merely because the data comes from the local
   observer store.

So: every chip and row in this plan is derived on the viewer's own machine. If
the project repo path does not resolve locally, chips silently do not render —
same behaviour as today, no error state. Making PR/CI state visible to members
who lack the checkout requires publishing it as a Nostr event (a new kind in
`buzz-core/src/kind.rs`); that is a **v2 phase**, explicitly out of scope here.

### D3 — One open PR per thread in the badge; closed count only in the drawer. ✅

One `gh pr list --state all` call per repo, mapped by `headRefName`, replaces the
current per-thread `gh` calls (`thread_github.rs:113-133` runs one `pr list`
plus one `pr view` per thread).

### D4 — No auto-delete in v1. ✅

Bulk actions are manual, behind a confirm dialog that names every path it will
touch, and reuse the existing dirty refusal
(`thread_workspace.rs:83-91`). Opt-in auto-prune is revisited only after the
list has proven trustworthy in daily use.

### D5 — Show disk size, computed lazily. ✅

Never inside the registry read: an 18 GB worktree makes `du` slow enough to
stall the whole list. Size is fetched per row on demand (Phase 3).

## Phases

| Phase | Delivers | Depends on | File |
| --- | --- | --- | --- |
| 0.5 | Pin `gh --repo` to the origin remote | — | **shipped** — commit `19220ade3`, `thread_github_target.rs` |
| 1 | `get_project_worktree_registry` + durable thread strip | 0.5 | [phase-01-worktree-registry.md](phase-01-worktree-registry.md) |
| 2 | Channel-timeline workspace badges | 1 | [phase-02-timeline-badges.md](phase-02-timeline-badges.md) |
| 3 | Channel header pill, Worktrees drawer, bulk cleanup | 1 | [phase-03-worktrees-drawer.md](phase-03-worktrees-drawer.md) |

Phase 2 and Phase 3 are independent of each other; both need Phase 1's command.

## Acceptance criteria (whole plan)

- Quit and reopen Desktop, open a project thread that ran days ago: branch, PR
  and CI row render without any agent running.
- The channel timeline shows branch + PR on project-thread summary rows.
- The channel header reports the worktree count; the drawer lists all 5 managed
  worktrees, flags `crew-02cc85801c3d` as Orphan, and never lists the two
  `.worktrees/` human checkouts as actionable.
- Removing a clean idle worktree from the drawer frees its disk; attempting it on
  a dirty worktree is refused with the existing message.
- `just desktop-ci` passes (`desktop-check`, `desktop-test`,
  `desktop-tauri-fmt-check`, `desktop-build`, `desktop-tauri-check`,
  `desktop-tauri-test`).

## Cross-cutting constraints

- **Every new module-level cache must be reset** in `resetCommunityState()`
  (`desktop/src/features/communities/useCommunityInit.ts:53-78`) — CLAUDE.md
  makes this mandatory; `resetProjectThreadGitHubStore()` at line 75 is the
  pattern to copy.
- **rem text tokens only** (`text-2xs`/`text-3xs` for chips). `pnpm check:px-text`
  fails on arbitrary literals.
- **No new `unwrap()`/`expect()`** in Rust production paths; no `unsafe`.
- **Keep files ≲200 lines**; split by responsibility, matching the existing
  `thread_workspace.rs` / `thread_workspace_git.rs` / `thread_github.rs` split.
- **Commit with `git commit -s`**; pre-commit `just desktop-tauri-fmt` fails in
  worktrees (gotcha #6) — run it from the main checkout.

## Unresolved questions

1. Does the drawer belong in the channel only, or also in the Projects screen
   (`desktop/src/features/projects/ui/`)? Plan assumes channel-only for v1.
2. Idle threshold for the "Idle" bucket — plan assumes 7 days, no activity, no
   PR, clean.
3. Should Phase 2 badges also render for project threads with zero replies? No
   summary row exists in that case (`threadPanel.ts:215-219` returns `null` when
   `descendantCount === 0`), so v1 skips them.
