# PR #9 — release v0.0.6 runtime test report (NuncioCrew desktop, real Tauri app)

Two runs are covered here:

- **Run 1** — full checklist at HEAD `ef7adaa94`.
- **Run 2 (re-run)** — focused re-test of the confirmation fix at HEAD `01e030225` → `d4a733471`
  (in-app AlertDialogs, button layout fix, PR/CI status colours).

App under test both times: the real Tauri dev build (`buzz-desktop`, `DISPLAY=:0`, Vite
`http://localhost:16431`) against the local relay. **No mock bridge / Playwright evidence is used.**
Agents: two Devin-ACP preset agents (Rex, Nova) created through the UI. The Pi/omp credential path
was **not** used — omp v17.2.2 dropped the legacy `~/.pi/agent/auth.json` and never completed ACP init.

Recordings:
- Run 1: `/home/ubuntu/screencasts/pr9-thread-integration-run2/pr9-thread-integration-run2-edited.mp4`
- Run 2: `/home/ubuntu/screencasts/pr9-confirm-rerun-final/pr9-confirm-rerun-final-edited.mp4`

## Result summary

| # | Checklist item | Run 1 | Run 2 (after fix) |
|---|----------------|-------|-------------------|
| 1 | Worktree cut from fetched remote tip, not local HEAD | passed | passed (re-observed: Base `40773ea…` / Source "Remote tip") |
| 2 | Strip renders 3-column rows; every cell opens a drawer | passed | **passed** (all six cells re-checked) |
| 3 | Agent mentioned in a reply joins handoff, working → done | passed | not re-tested |
| 4 | No PR on branch → GitHub row collapses | passed (row omitted; no literal "No PR yet" label — documented as intended) | — |
| 5 | Issue / PR / CI cells + drawers populate | passed (detection); opening a PR *from the thread* has no in-app action → untested | passed again, now with colour-coded status |
| 6 | `Remove worktree` refuses while dirty, succeeds clean | passed | **passed** |
| 7 | `Delete branch` / `Close PR` (and Remove worktree) confirm before acting | **FAILED** — no dialog at all | **PASSED** — dialog for all three; Cancel inert, Confirm acts |
| 8 | Preset harness agent → vendor avatar | passed | — |
| 9 | Ctrl +/- zoom scales all strip text | passed | — |
| 10 | Switch communities → no PR data leaks | untested — app crashes on community switch (confirmed pre-existing on `origin/main`, out of scope) | still untested |
| — | Workspace button labels overlapping (user report) | n/a | **passed** — no overlap; truncate with "…" when the panel is narrow |
| — | PR/CI status colours | n/a | **passed** |

Everything the re-run was asked to prove now passes. The only open item is #10, which the lead
confirmed reproduces on `origin/main` and is out of scope for this PR.

---

# Run 2 — confirmation dialogs (HEAD 01e030225 → d4a733471)

Fixture: scratch clone `/home/ubuntu/scratch/work` (local `main` at `b52909c`, `origin/main` at
`8edc761`) for the worktree/branch tests, and the crew repo for one throwaway draft PR
(**#12, "TEST — do not merge …"**, user-approved) for the Close PR test.

## Item 6 — dirty worktree still refuses

An untracked file in the thread worktree ⇒ `Changes: Uncommitted changes`, both destructive buttons
disabled, clicking Remove worktree opens **no** dialog and `git worktree list` still shows the
worktree.

![dirty worktree, buttons disabled](https://app.devin.ai/attachments/a3c0948d-71dc-4003-9bd3-58b1e828c297/ss_2ec05ad6.png)

## Item 7a — Remove worktree confirms

Dialog **"Remove worktree?" / "Remove this clean thread worktree? The branch will remain."** with
Cancel + Confirm.

![remove worktree confirmation](https://app.devin.ai/attachments/1f075286-0499-46e6-b157-e8cb48444a44/ss_7a0e23f0.png)

```
after Cancel :  /home/ubuntu/scratch/.buzz-worktrees/work-1eab81a205b7   7d40f4c [buzz/1eab81a205b7]   <- still there
after Confirm:  ls: cannot access '.../work-1eab81a205b7': No such file or directory
                git branch --list 'buzz/*'  ->  buzz/1eab81a205b7        <- branch correctly survives
```

## Item 7b — Delete branch confirms

Dialog **"Delete branch?" / "Delete this local and remote thread branch?"** with Cancel + Confirm.

![delete branch confirmation](https://app.devin.ai/attachments/97e2324a-15c6-4368-a157-77b2179baf18/ss_2341f842.png)

```
after Cancel :  git branch --list 'buzz/*'  ->  buzz/1eab81a205b7   <- untouched
after Confirm:  git branch --list 'buzz/*'  ->  (empty)             <- deleted
```

## Item 7c — Close PR confirms

Dialog **"Close pull request?" / "Close pull request #12?"** with Cancel + Close PR.

![close PR confirmation](https://app.devin.ai/attachments/7ec113cf-2033-4900-a8d0-83c470ad3407/ss_6a563704.png)

```
after Cancel : gh api repos/Nuncio-hq/crew/pulls/12 --jq '{number,state}' -> {"number":12,"state":"open"}
after Confirm: gh api repos/Nuncio-hq/crew/pulls/12 --jq '{number,state}' -> {"number":12,"state":"closed"}
```

The PR cell updates to a red **"Closed · PR #12"** and the drawer badge switches to `CLOSED`:

![PR cell closed, CI passing](https://app.devin.ai/attachments/26039deb-46fa-44a1-8f7c-b779b9e364f9/ss_zoom_1e57037c.png)

## Item 2 — all six cells open their drawers

Task, Workspace, Handoff, Issue, Pull request and CI each opened their matching drawer. CI drawer
with the emerald `PASSING` badge and five real GitHub checks:

![CI drawer passing](https://app.devin.ai/attachments/eaccc923-7a3e-425d-b3e3-31a24b1a3908/ss_156ff2ba.png)

## New status colours

Before the PR was closed the cells showed **Draft · PR #12** (muted) and **Pending / 0/1 passed**
(amber); after CI finished, **Passing** (emerald); after closing, **Closed** (destructive red — see
the screenshot above).

![draft + pending colours](https://app.devin.ai/attachments/8e4b6b2d-a269-45ce-8dd0-7b1b5364c15f/ss_zoom_e1d480da.png)

## Workspace button layout (user-reported overlap)

No overlap at any width I tried. At the default (narrow) thread panel the labels truncate with an
ellipsis; widening the panel shows them in full.

| Default panel width | Widened panel |
|---|---|
| ![narrow buttons](https://app.devin.ai/attachments/d5ac26d2-bf26-4724-8125-e27c59c8915e/ss_zoom_0e15bda7.png) | ![wide buttons](https://app.devin.ai/attachments/4256a743-e100-4223-bd24-ab11fe971975/ss_zoom_de79f8e8.png) |

## Interruption during the re-run (resolved)

Midway through, opening a thread with a GitHub row blanked the app with
`ReferenceError: Can't find variable: statusClassName` — uncommitted work-in-progress in
`ProjectThreadIntegrationCell.tsx` was being served by Vite HMR (the prop was declared in the type
but not destructured). The lead fixed it in `d4a733471`, and the Close PR test was then completed on
the fixed build. Screenshot of the crash state, for the record:

![statusClassName crash](https://app.devin.ai/attachments/c66fe192-f4b5-4ddd-af1b-33dc2a039e3d/ss_dd125adc.png)

## Cleanup (run 2)

- PR #12 closed **via the app's Close PR button**, verified `state: closed`.
- Remote + local branch `buzz/34dea2a09780` deleted; thread worktrees removed from both repos.
- Open PRs afterwards: only #9 and the pre-existing #11. PR #9, `buzz/eb791333c0ee` and `main`
  were never touched.

---

# Run 1 — full checklist (HEAD ef7adaa94)

Kept for reference; only item 7 changed since.

Setup note that still applies: managed agent pairs connect to **`ws://127.0.0.1:3000`**. On the
`localhost:3000` community every pair logs `discovered 0 channel(s) — agent will sit idle` and
threads never leave `Preparing`; all testing must happen in the `127.0.0.1:3000` community.

- **1 · Remote tip base** — Workspace drawer reported Base `8edc761…` / Source "Remote tip" while
  the clone's HEAD was `b52909c`; `git log` in the worktree confirmed `agent commit → 8edc761 → b52909c`.
- **2 · Strip + drawers** — Task/Workspace/Handoff in one 3-column row, plus Issue/Pull request/CI
  when a PR exists; every cell opened its drawer.
- **3 · Reply mention** — `@Nova` appeared as "Added in a reply", went Working → Done next to Rex.
- **4 · No PR** — the GitHub row is omitted entirely (no literal "No PR yet" label; documented as intended).
- **5 · PR/CI cells** — populated from real PR data with five live CI checks. There is no in-app
  "open a PR from this thread" action, so that half stays untested.
- **6 · Dirty/clean worktree removal** — refused while dirty, succeeded when clean.
- **7 · Confirmations** — FAILED at the time: `window.confirm` mapped to a `dialog.confirm` IPC
  command missing from the app capabilities (`dialog.confirm not allowed` in the dev log), so the
  guard returned a truthy Promise and all three destructive actions ran unconfirmed. **Fixed and
  re-verified in run 2.**
- **8 · Vendor avatar** — Devin logo shown for both preset agents.
- **9 · Zoom** — all strip and drawer text scaled on Ctrl +/-.
- **10 · Community isolation** — untested: clicking a community in the left rail kills the app
  (reproduced twice; lead confirmed it also reproduces on `origin/main`).
- **Extra** — thread workspace/PR state is in-memory only, so after an app restart the strip reverts
  to "Preparing" and the GitHub row disappears. Documented as intended by the plan docs.
