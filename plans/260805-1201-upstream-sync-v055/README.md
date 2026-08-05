# Upstream sync — Buzz Desktop v0.5.3 → v0.5.5

Status: **planned, not started**. Owner: CTO. Created 2026-08-05.

## Sync target (pinned to a release, not to main)

| | Value |
|---|---|
| Current pin | `desktop-v0.5.3` / `3a96acea0` (`docs/crew/upstream-buzz.json`) |
| New pin | `desktop-v0.5.5` / `8342dfcc5` |
| Published release | "Buzz Desktop v0.5.5", Latest, 2026-08-05T01:23:40Z |
| Commits between | 86 (73 non-merge) |
| Merge conflicts | 30 files |

**Rule:** merge the release tag `desktop-v0.5.5`, never `upstream/main`. At the
time of planning the two are the same commit and `desktop-v0.5.5..upstream/main`
is empty, but main drifts and the tag does not. This matches Crew's existing
convention — the current pin is also a release tag.

**Mobile caveat:** `mobile-v0.8.0-rc.1` / `rc.2` are git tags with **no
published GitHub release** (`gh release list --repo block/buzz` shows desktop
releases only). Merging the desktop release tag necessarily brings that mobile
tree with it — it is one commit, and filtering it would fork upstream code,
which the thin-fork rules forbid. Crew CI already excludes Flutter mobile
(`docs/crew/CI.md`), so the exposure is bounded. Named here so it is a known
acceptance, not a surprise.

## What upstream shipped

### In v0.5.5

| Change | PR | Relevance to Crew |
|---|---|---|
| Multi-repo projects (NIP-MP, `KIND_PROJECT` 30621) | #4671 | **Highest.** Reshapes the type Crew builds on |
| Buzz Term docked in channel workspace | #4724 | Pure gain — Crew has no in-app terminal |
| Buzz entity links (`buzz://pr\|issue\|repo`) | #4695 | Additive; does not touch Crew's GitHub chips |
| Close reconnect gaps (no more CMD+R) | #4737 | Pure gain |
| Huddle redesign + voice polish | #4281, #4694 | Neutral |
| Relay perf: channel-id index, read-replica membership | #4647, #4124 | Neutral; adds migration `0027` |
| Paste composer text without formatting | #4801 | Neutral |
| Sidebar observed-unread across reload | #3976 | Neutral |

### In v0.5.4

| Change | PR | Relevance to Crew |
|---|---|---|
| Security: nostr crates for RUSTSEC-2026-0225..0232 | #4392 | **Take.** Reason not to defer the sync |
| Security: nostr-relay-pool for RUSTSEC-2026-0224 | #4139 | **Take** |
| Buzz Term shipped | #4347 | Prerequisite for #4724 |
| `buzz projects` CLI (kind 30621) | #4020 | Pairs with #4671 |
| Kubernetes backend plugin (new crate) | #4289 | Out of Crew scope; arrives with the tree |
| Agent Trading Cards | #3278 | Neutral |
| ACP system prompt via `_meta.systemPrompt` | #4395 | Check against Crew ACP modules |

## The structural change that drives the work

Upstream split the type Crew calls `Project`
(`desktop/src/features/projects/projectModels.ts`):

- `Repository` (kind `30617`) — `cloneUrls`, `dtag`, `defaultBranch`, `repoAddress`
- `Project` (kind `30621`) — `repositoryAddresses[]`, `repositories[]`,
  `primaryRepositoryAddress`, `legacy`

Crew's `Project` (`hooks.ts:69`) carries `cloneUrls` and `localWorkspacePath`,
which belong to upstream's **`Repository`**. So **Crew's "Project" is
upstream's `Repository`** — Crew's per-repo features are semantically correct
under NIP-MP and need re-homing, not rewriting.

`repositoryToLegacyProject()` (`projectModels.ts:368`) wraps a bare `30617`
repository into a synthetic project with `legacy: true`. Existing Crew projects
therefore survive the sync as single-repository legacy projects. Ride this ramp;
do not build another.

**Decision (approved):** when a project has several repositories, Crew binds a
thread worktree to `primaryRepositoryAddress`.

## Phases

| # | Phase | Depends on |
|---|---|---|
| 0 | Land Phase 3 of `260804-2040-gh-path-pr-visibility` and merge | — |
| 1 | [Merge the release tag, clear mechanical conflicts](phase-01-mechanical-merge.md) | 0 |
| 2 | [Re-home Crew per-repo surfaces onto `Repository`](phase-02-rehome-onto-repository.md) | 1 |
| 3 | [Add three upstream guards to the Upstream Sync lane](phase-03-upstream-sync-ci-guards.md) | 1 |
| 4 | [Move the pin, correct the docs](phase-04-pin-and-docs.md) | 2, 3 |

### Phase 1 — mechanical conflicts

30 conflicted files. Default rule: **take upstream's version, then re-apply the
Crew delta on top.** Do not hand-merge line by line.

| Cluster | Files |
|---|---|
| `desktop/src/features/messages/ui` | 7 |
| `desktop/src/features/projects` (+ `ui`, `lib`) | 6 |
| `desktop/src/features/channels` (+ `ui`) | 4 |
| `mobile/lib` + `mobile/test` | 5 |
| `desktop/src-tauri/src` (+ `commands`) | 4 |
| `crates/buzz-acp/src`, `crates/buzz-sdk/src` | 3 |

The three `features/projects` files with real Crew logic — `hooks.ts`,
`repoSyncHooks.ts`, `useProjectsRepoSnapshots.ts`, plus
`lib/projectLocalRepos.ts` and `ui/ProjectsView.tsx` — carry into Phase 2 rather
than being forced closed here.

### Phase 2 — re-home onto `Repository`

- Move Crew's `localWorkspacePath` / `localWorkspaceStatus` from Crew's
  `Project` onto upstream's `Repository`.
- `lib/projectLocalRepos.ts:32` uses `project.cloneUrls[0]`. Under the new model
  the caller picks a repository from `project.repositories[]` first, then reads
  that repository's `cloneUrls`.
- Worktree registry (`project_worktree_registry.rs`) and GitHub target
  (`thread_github_target.rs:5`) resolve per-checkout today. Give them the
  `primaryRepositoryAddress` rule.
- `buzz-location` tag stays on kind `30617` — already per-repository, already
  correct under NIP-MP. No event-shape change.

### Phase 3 — CI guards (approved)

Add to the manual `NuncioCrew Upstream Sync` lane, copied from upstream
`ci.yml`:

1. workspace-profile `kind:9033` gate tests
2. NIP-MP coordinate-deletion guard (never delete a head newer than the tombstone)
3. `e2e_project`

These are additive to the approved path-gated `Desktop Rust` job (issue #41) and
do not touch its design.

### Phase 4 — pin and docs

- `docs/crew/upstream-buzz.json` → `0.5.5` / `desktop-v0.5.5` / `8342dfcc5`.
- `docs/crew/DECISIONS.md` D-003 and D-010 say "Project" where they mean kind
  `30617` = upstream's *repository*. Add a clarifying sentence. **Do not rewrite
  the decisions** — the mechanism they describe is still correct.
- Record the `primaryRepositoryAddress` binding as a new decision.

## Acceptance criteria

- `docs/crew/upstream-buzz.json` names `desktop-v0.5.5` / `8342dfcc5`.
- `git merge-base --is-ancestor desktop-v0.5.5 <sync-branch>` succeeds.
- `NuncioCrew Gate` green on the sync branch.
- An existing single-repository Crew project still opens, shows its local path,
  and lists its thread worktrees after the merge.
- No Crew code reads `project.cloneUrls` — it reads `repository.cloneUrls`.

## Risks

| Risk | Mitigation |
|---|---|
| Crew's `features/projects` delta does not survive the multi-repo reshape | Legacy ramp keeps single-repo projects working; Phase 2 is scoped to re-homing |
| Mobile arrives at RC quality | Crew CI excludes Flutter; accepted above |
| 30-file merge lands broken in a way CI does not catch | Crew CI is macOS-desktop only by design (D-017); Phase 3 guards close part of it |

## Not verified

- No build or test has been run for this plan. All claims come from reading
  source at `desktop-v0.5.5` (`8342dfcc5`) and `origin/main` (`971dffdf1`).
- The "take upstream, re-apply Crew delta" rule is a proposal, not a measured
  cost. The 30 conflicting files have not each been read.

## Open questions

1. Thin-fork budget: `UPSTREAM-SYNC.md` states one route-registration edit and
   one navigation-entry edit, but 113 upstream files are currently modified
   (largest: `crates/buzz-acp/src/lib.rs` +897/−96). Restate the budget to match
   reality, or reconcile decision by decision? Not blocking this sync.
2. Should Crew's GitHub PR/CI chips also render on upstream's preview-card
   surface, now that entity links exist? Deferred until after the sync.
