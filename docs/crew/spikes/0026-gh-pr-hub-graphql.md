# Spike 0026 — GitHub PR hub `gh` data plane (#193)

- **Status:** PASS
- **Date:** 2026-08-13
- **Issue:** [#193](https://github.com/Nuncio-hq/crew/issues/193)

## Question

Can one `gh api graphql` query return PR body + comments + review threads +
commits + files (`viewerViewedState`) + check runs with re-runnable workflow
run ids under default `gh` auth? Do `markFileAsViewed`, `gh run view
--log-failed`, and local `git diff` vs `gh pr diff` hold for the hub?

## Decision affected

D-057 / #193 — two-tier GitHub PR hub on the `gh` data plane with a
forge-neutral provider trait. GitLab/`glab` is not this issue.

## Hypothesis

GitHub's GraphQL schema already exposes the batch on `PullRequest` plus
`statusCheckRollup.contexts.nodes` (a connection — fragments must spread on
`nodes`, not the connection). `viewerViewedState` / `markFileAsViewed` exist
on standard scopes. `gh run view --log-failed` is TSV (`job\\tstep\\tline`)
and can be tailed. Local `git diff --find-renames` against the PR base
matches `gh pr diff` file lists; rename/binary need dedicated parsers.

## Scope

- Live `gh` 2.91.0 against `Nuncio-hq/crew` (authenticated `github.com`)
- In-repo `gh_cli.rs` / `thread_github.rs` wrappers (not production hub code)
- Files under `docs/crew/spikes/assets/0026-gh-pr-hub-graphql/`
- Time: one cloud-agent session

## Exclusions

- GitLab / `glab`
- Webhooks
- Promoting spike query files into the Tauri crate unchanged
- Inventing GraphQL fields not returned by introspection or a live query

## Pass criteria

Each of (a)–(d) is labeled with live or in-repo evidence. Schema fields are
cited, not guessed. Unknowns are explicit.

## Fail criteria

Claiming a field that introspection/live query rejected. Designing the hub
around stacked `gh pr view` calls after a working batch query.

## Environment

- Commit: Crew `main` at `5ffd885a3` (branch start)
- OS: Linux cloud agent
- `gh version 2.91.0 (2026-04-22)`
- Auth: logged in to github.com (`gh auth status` succeeded; token not recorded)
- Live GitHub: **available**

## Method

1. Introspect `PullRequestChangedFile`, `FileViewedState`, `MarkFileAsViewedInput`,
   `StatusCheckRollupContextConnection`, `CheckRun`, `WorkflowRun`.
2. Run one GraphQL query (`detail.graphql`) against PR #202.
3. `markFileAsViewed` then `unmarkFileAsViewed` on `crates/buzz-acp/src/acp.rs`.
4. `gh run view <failed> --log-failed` and inspect TSV shape.
5. `git fetch` PR #202 and compare `git diff --find-renames --name-only
   origin/main...` to `gh pr diff 202 --name-only`. Local throwaway repo for
   rename + binary (no rename/binary files on PR #202).

## Results

### (a) One GraphQL batch — PASS (live)

First query failed: fragments on `CheckRun`/`StatusContext` cannot spread
inside `StatusCheckRollupContextConnection`. Fix: `contexts(first: N) { nodes {
... on CheckRun { … } ... on StatusContext { … } } }`.

Live query against PR #202 (`detail-pr-202.json`):

| Field | Observed |
| --- | --- |
| `title` / `body` / `state` | present (`MERGED`, body 3861 chars) |
| `id` | `PR_kwDOTnxGBc7-jTvR` (needed by viewed mutation) |
| `comments` / `reviews` / `reviewThreads` | connections with `nodes` (0 on this PR) |
| `history: commits(last: 20)` | 2 commits (`oid`, `messageHeadline`, `author`) |
| `files.nodes[]` | 21 files; `path`, `additions`, `deletions`, **`viewerViewedState`** (`UNVIEWED`) |
| `head: commits(last: 1).statusCheckRollup.contexts.nodes` | 14 `CheckRun`s |
| workflow run id | `checkSuite.workflowRun.databaseId` (e.g. `31696638935`) — this is the `gh run rerun` id |
| merge strategies | `mergeCommitAllowed` / `squashMergeAllowed` / `rebaseMergeAllowed` all `true` |
| `rateLimit` | `{ remaining: 4853, resetAt: "2026-08-13T12:15:26Z" }` |

GraphQL **aliases** (`history` + `head`) are required: two `commits` fields
cannot share a name.

### (b) `viewerViewedState` mutation — PASS (live)

Introspection: `PullRequestChangedFile.viewerViewedState` → enum
`FileViewedState` = `VIEWED | UNVIEWED | DISMISSED`. Mutations
`markFileAsViewed` / `unmarkFileAsViewed` take `{ pullRequestId, path }`.

Live mark then unmark on PR #202 file `crates/buzz-acp/src/acp.rs` under
default `gh` scopes: `VIEWED` then `UNVIEWED` (`mark-viewed.json`,
`unmark-viewed.json`).

### (c) `gh run view --log-failed` — PASS (live, with parser notes)

Failed run `31697439401` (Sprig image). Output is TSV:

```text
<job name>\t<step name>\t<timestamp> <line>
```

This run labeled every line `UNKNOWN STEP` (3272 lines, two jobs). Failed
steps are still findable via `##[error]` in the line body. Parser contract:
split on the first two tabs, group by `(job, step)`, prefer `##[error]`
clusters, keep ~last 50 lines per group, bound total size. Do not dump the
raw 3k-line blob into the UI.

Excerpt: `log-failed-excerpt.txt`. Full capture was not committed.

### (d) Local `git diff` vs `gh pr diff` — PASS (live file list + local rename/binary)

PR #202: local `git diff --find-renames --name-only origin/main...` and
`gh pr diff 202 --name-only` both listed **21** identical paths. Per-file
unified headers match (`diff --git a/… b/…`).

PR #202 had **no renames or binaries**. Local throwaway repo evidence
(labeled as such, not a GitHub PR):

```text
numstat:  0  0  old.txt => new.txt
          -  -  blob.bin
name-status: R100 old.txt  new.txt
             M    blob.bin
patch:    similarity index 100% / rename from / rename to
          Binary files a/blob.bin and b/blob.bin differ
```

Hub diff parser must accept `old => new` numstat paths (use the new path)
and binary rows (`-` counts). `project_git_diff::parse_numstat` currently
takes the raw third column and would pass `old.txt => new.txt` to
`git diff -- path`, which misses the rename.

## Edge cases observed

- Rollup `contexts` is a **connection**; spreading inline fragments on the
  connection is a hard GraphQL error.
- Merged PRs still accept `markFileAsViewed` (we unmarked immediately).
- `--log-failed` may not populate real step names (`UNKNOWN STEP`).
- `reviewDecision` was `null` on the merged PR (not an empty string).
- Check runs in one workflow share one `workflowRun.databaseId`; re-run is
  per workflow run, not per check name.

## Limitations

- Pagination: `files(first: 50)` / `commits(last: 20)` / `contexts(first: 40)`
  will truncate large PRs. Hub should surface truncation, not silently drop.
- No live GitHub PR with a rename/binary was available; those shapes are
  local-git labeled evidence.
- Rate-limit path was not live-triggered (`remaining` was 4853). Error
  classification uses `RATE_LIMITED` / “rate limit” plus `rateLimit.resetAt`.

## Verdict

**PASS.** The hub can use one GraphQL query, GitHub-native viewed state,
in-app log tails from TSV, and worktree `git diff` with `gh pr diff`
fallback. Production query must use `contexts.nodes`.

## Follow-up test contract

- Parse the slim fixture (`detail-pr-202.slim.json`) into forge-neutral types
  including check `runId`.
- Rate-limit and cli-missing/cli-failed classification.
- Diff source: worktree present vs missing/evicted → API.
- Numstat rename `old => new` and binary `-\\t-`.
- Log-tail last-N + `##[error]` grouping.
- `markFileAsViewed` request shape (path + pullRequestId).

## Cleanup

- Deleted 3272-line log capture; kept excerpt.
- Deleted `refs/tmp/spike-pr-202`.
- Unmarked the viewed file after the mutation probe.
- Query/fixture files remain under `docs/crew/spikes/assets/0026-gh-pr-hub-graphql/`.
