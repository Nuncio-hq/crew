# PR #9 re-run — lifecycle confirmation dialogs (HEAD 01e030225)

Scope: checklist item 7 (Remove worktree / Delete branch / Close PR must confirm), plus a smoke of
item 6 (dirty refusal) and item 2 (drawers still open) after the refactor. Everything else from the
previous run stands.

Code paths that define the expected UI (read before planning):
- `desktop/src/features/messages/ui/ProjectThreadWorkspaceDetails.tsx:150-228` — Remove worktree /
  Delete branch set `pendingAction`; a Radix `AlertDialog`
  (`data-testid="project-thread-workspace-confirm"`) renders the title ("Remove worktree?" /
  "Delete branch?"), the description, a **Cancel** button and a destructive **Confirm** button
  (`project-thread-workspace-confirm-action`). `run()` no longer calls `window.confirm`.
- `desktop/src/features/messages/ui/ProjectThreadGitHubDetails.tsx:186-227` — Close PR sets
  `confirmOpen`; dialog `project-thread-close-pr-confirm` titled "Close pull request?" with body
  "Close pull request #N?", **Cancel** and destructive **Close PR**
  (`project-thread-close-pr-confirm-action`).

Preconditions already satisfied: app rebuilt at 01e030225 and running; active community is the
`127.0.0.1:3000` one (agents only discover channels there); scratch clone `/home/ubuntu/scratch/work`
sits at `b52909c` with `origin/main` at `8edc761`.

## T1 — Dirty worktree still refuses (item 6 regression smoke)
Mention @Rex in `#work project` to provision a thread worktree, create an untracked file inside the
worktree, reopen the Workspace drawer.
- Pass: drawer reads `Changes: Uncommitted changes`; **Remove worktree** and **Delete branch** are
  visibly disabled; clicking Remove worktree opens **no** dialog and `git worktree list` still lists
  the worktree.
- Fail: buttons enabled, dialog opens, or the worktree disappears.

## T2 — Remove worktree: Cancel is inert, Confirm removes (item 7 + item 6 clean half)
Delete the untracked file, reopen the drawer, click **Remove worktree**.
- Pass (dialog): an in-app dialog titled **"Remove worktree?"** with the body "Remove this clean
  thread worktree? The branch will remain." and Cancel / Confirm buttons is visible in a screenshot.
- Pass (cancel): after clicking **Cancel** the dialog closes, no toast appears, and
  `git worktree list` still contains `.buzz-worktrees/work-<id>`; the directory still exists.
- Pass (confirm): reopening the dialog and clicking **Confirm** shows the success toast and the
  worktree is gone from `git worktree list` **and** from disk, while the branch `buzz/<id>` still
  exists in `git branch`.
- Fail: no dialog, Cancel removes anything, or Confirm does not remove.

## T3 — Delete branch: Cancel is inert, Confirm deletes (item 7)
In the same drawer click **Delete branch**.
- Pass (dialog): dialog titled **"Delete branch?"** with body "Delete this local and remote thread
  branch?" plus Cancel / Confirm, visible in a screenshot.
- Pass (cancel): `git branch --list 'buzz/<id>'` still returns the branch after Cancel.
- Pass (confirm): after Confirm the branch is absent from `git branch`.
- Fail: no dialog, Cancel deletes, or Confirm does not delete.

## T4 — Close PR: Cancel is inert, Confirm closes (item 7)
Provision a second thread in `#crew project` (repo `/home/ubuntu/repos/crew`), push its
app-generated `buzz/<id>` branch and open ONE throwaway **draft** PR titled
"TEST — do not merge …" (user-approved; PR #9 / `main` untouched). Open the thread's Pull request
drawer and click **Close PR**.
- Pass (dialog): dialog titled **"Close pull request?"** with body "Close pull request #N?" plus
  Cancel / Close PR, visible in a screenshot.
- Pass (cancel): after Cancel, `gh api repos/Nuncio-hq/crew/pulls/N --jq .state` still returns
  `open`.
- Pass (confirm): after Confirm the PR cell shows the PR as closed and the same `gh` query returns
  `closed`.
- Fail: no dialog, Cancel closes the PR, or Confirm leaves it open.

## T5 — Drawers still open (item 2 smoke)
Click each strip cell (Task, Workspace, Handoff, and Issue / Pull request / CI when the PR exists).
- Pass: every cell opens its matching drawer with a heading that matches the cell label.
- Fail: any cell is inert or opens the wrong drawer.

## Cleanup
Close the throwaway PR through the app (T4 confirm), delete its remote branch, remove leftover
worktrees/branches from both repos, and verify only pre-existing PRs remain open.
