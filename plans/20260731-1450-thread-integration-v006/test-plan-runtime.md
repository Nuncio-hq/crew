# PR #9 — real Tauri runtime test plan (release v0.0.6 gate)

Environment (verified during setup):
- Real Tauri dev app on DISPLAY=:0 (Vite http://localhost:16431, relay ws://localhost:3000).
- Active community must be the one whose host matches the relay URL agent pairs use
  (`ws://127.0.0.1:3000`). On the `localhost:3000` community the pairs log
  `discovered 0 channel(s) — agent will sit idle` and never take a turn.
- Two Devin-preset agents created: Rex, Nova (harness `Devin`, model claude-opus-5-low-fast).
- Scratch git repo: local clone `/home/ubuntu/scratch/work` at `b52909c` (A: base),
  remote `origin/main` at `8edc761` (C: remote tip only) — clone is deliberately behind.
- Approved by user: exactly ONE throwaway draft PR in Nuncio-hq/crew from a scratch branch,
  title prefixed "TEST — do not merge", closed via the app and branch deleted afterwards.

All evidence must come from the running app via computer use. No mock bridge, no e2e harness.
Record the whole run; annotate each item with test_start + consolidated assertion.

## 1. Worktree cut from remote tip (not local HEAD)
Open a new Project thread in #work project by mentioning @Rex; open the Workspace cell.
Pass: drawer shows branch `buzz/<id>`, worktree path under `/home/ubuntu/scratch/.buzz-worktrees/`,
and a base revision equal to remote tip `8edc761…` (or "Remote tip"/"origin/main" source label),
NOT local HEAD `b52909c…`. Cross-check with `git log` in the worktree.
Fail: base is local HEAD or source label says local fallback.

## 2. Strip = two 3-column rows, every cell opens a drawer
Pass: row 1 is Task | Workspace | Handoff in a 3-column grid (not stacked cards); clicking each
cell opens the matching drawer with a title equal to the cell label.
Fail: stacked layout, missing cell, or a cell that does not open its drawer.

## 3. Agent mentioned in a reply joins the handoff list
Reply in the thread mentioning @Nova. Open Handoff.
Pass: Nova appears in the handoff list, shows `working` while the turn runs and `done` after it
completes; Rex remains listed.
Fail: reply-mentioned agent never appears or stays queued after it replies.

## 4. No PR on the branch → row 2 collapses to "No PR yet"
Pass: while the thread branch has no PR, the second row is collapsed/absent and the UI reads
"No PR yet" (or equivalent single-line empty state).
Fail: empty issue/PR/CI cells rendered as a full row, or stale PR data.

## 5. Open a PR from the thread → issue/PR/CI cells populate, drawers show history
Use the approved throwaway draft PR in Nuncio-hq/crew (scratch branch, "TEST — do not merge").
Pass: after the PR exists, row 2 renders with PR (and CI) cells populated; opening the PR cell
drawer shows PR metadata/history; CI cell reflects a real check state.
Fail: cells stay empty, show wrong PR, or drawer is empty.

## 6. `Remove worktree` refuses while dirty, succeeds once clean
Create an uncommitted file inside the thread worktree, refresh, invoke Remove worktree.
Pass: the action is blocked/refused with an "uncommitted changes" style message; after cleaning
the worktree, Remove worktree succeeds and the worktree directory is gone on disk.
Fail: dirty removal succeeds, or clean removal fails.

## 7. `Delete branch` and `Close PR` confirm before acting
Pass: each action opens a confirmation; cancelling leaves state unchanged (branch still present,
PR still open); confirming performs the action.
Fail: either action executes without a confirmation step.

## 8. Preset harness agent shows vendor avatar
Pass: agent created on the `Devin` preset harness shows the Devin vendor avatar in the Agents
list/persona card and in the thread/handoff surfaces (not a generic placeholder).
Fail: generic avatar or broken image.

## 9. Ctrl +/- zoom scales all strip text
Pass: at 2 steps zoom-in and 2 steps zoom-out, all strip labels/values and drawer text scale with
the rest of the app; nothing stays frozen at its original px size or clips.
Fail: any strip text does not scale.

## 10. Community switch → no PR/workspace data leaks
Switch between the two local communities and reopen Project threads.
Pass: each thread shows only its own workspace/GitHub state; no PR row or drawer content from the
other community appears.
Fail: any cross-community leakage or stale row.

## Cleanup
Close the throwaway PR via the app's Close PR button, delete the scratch branch, and verify no
scratch branch or open PR remains in Nuncio-hq/crew.
