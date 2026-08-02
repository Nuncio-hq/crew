# Channel-level worktree management — plan index

Date: 2026-08-02 · Owner: Oscar · Author: Claude Opus
Source brainstorm: [`plans/reports/brainstorm-260802-1417-channel-worktree-management.md`](../reports/brainstorm-260802-1417-channel-worktree-management.md)
Status: **Phase 0.5 shipped; Phases 1–2 handed to Cursor Grok High Fast; Phase 3 queued**
UI reference: [`ui-preview.html`](ui-preview.html) — open it in a browser; the
controls at the bottom switch layout, naming and density.

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

### D3 — A thread carries a **list** of pull requests. ⚠️ revised

Original: "one open PR in the badge, closed count only in the drawer." Oscar
corrected the premise — a thread branch routinely accumulates several PRs
(superseded attempt, merged part, live follow-up). So the registry returns
`pull_requests: Vec<…>` and the badge renders the two highest-ranked plus a
`+N` overflow chip.

Rank order: `open` → `draft` → `merged` → `closed`, then descending number.

Cost is unchanged: still one `gh pr list --state all` per repo, mapped by
`headRefName`, replacing the current per-thread pair of calls
(`thread_github.rs:113-133` runs one `pr list` plus one `pr view` per thread).

### D4 — No auto-delete in v1. ✅

Bulk actions are manual, behind a confirm dialog that names every path it will
touch, and reuse the existing dirty refusal
(`thread_workspace.rs:83-91`). Opt-in auto-prune is revisited only after the
list has proven trustworthy in daily use.

### D5 — Show disk size, computed lazily. ✅ (unchanged)

Never inside the registry read: an 18 GB worktree makes `du` slow enough to
stall the whole list. Size is fetched per row on demand (Phase 3).

### D6 — Inline layout is **A · chips**. ✅ decided by Oscar, 2026-08-02

Reviewed in [`ui-preview.html`](ui-preview.html) against B (a separate status
line) and C (a bordered card). A keeps the timeline flat: the chips join the
existing `N replies · last reply …` row, so a channel of project threads gains
no vertical height at all.

Consequences, both already handled below:

- The overflow chip (`+N`) is what makes A survive the multi-PR case of D3.
- A zero-reply thread has no summary row to join, so it renders nothing — see
  the resolved question 3.

### D7 — The worktree chip shows a **derived thread label**, not the branch id. ✅

An ACP session title cannot name a worktree: it is composed as
`"<agent display name> · #<channel>"` (`metadata.rs:45`, `config.rs:616-632`),
pushed one-way into `session/new` as `_meta.sessionTitle` (`acp.rs:649-659`),
and never read back (`acp.rs:661-664`). It is per *agent × channel*, so every
thread in one channel would share it; and with several agents tagged there are
several sessions, so "the first agent's title" also depends on who replies
first. `docs/crew/STATE.md:45` already lists semantic branch renaming as out of
scope for this slice.

So identity and label are separated:

| | Value | Where it lives |
| --- | --- | --- |
| Identity | `buzz/<12hex>` | git — load-bearing for `validate_target`, PR head refs, and the Phase 3 path guard. Never renamed. |
| Label | first prose line of the thread root | derived at render time, stored nowhere |

Derivation is per-viewer and needs no agent to have run, so it is stable across
machines and across how many agents were tagged.

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

## Resolved questions

1. **Drawer lives in the channel only for v1.** The Projects screen
   (`desktop/src/features/projects/ui/`) would serve a cross-channel view; that
   is a later slice, not a blocker for anything here.
2. **Idle = clean, no open PR, no commit for 7 days.** A threshold only gates
   which bucket a row lands in; every removal is still an explicit click behind
   a confirm dialog (D4), so the cost of getting the number slightly wrong is
   cosmetic.
3. **Zero-reply project threads render nothing.** `buildTimelineThreadSummary`
   returns `null` when `descendantCount === 0` (`threadPanel.ts:215-219`), and
   D6 puts the chips *inside* that row — a standalone row would be a second
   render path for a state that barely occurs, since a worktree only exists
   after an agent has run and an agent run produces a reply. The gap is the few
   seconds of a first turn before its first reply lands.

No open questions.
