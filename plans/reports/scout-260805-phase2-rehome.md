# Phase 2 post-merge scout

## Result

Phase 2 is not yet implemented. Upstream's NIP-MP `Project`/`Repository` split
is present, and `hooks.ts` has already stopped declaring a second `Project`
type. Crew's local-workspace fields and most clone reads are still attached to
the old single-repository surface.

## 1. Current model shape

- `desktop/src/features/projects/projectModels.ts:8-25`: `Repository` has
  `cloneUrls`, `defaultBranch`, and `repoAddress`, but **no**
  `localWorkspacePath` or `localWorkspaceStatus`.
- `desktop/src/features/projects/projectModels.ts:27-44`: `Project` has
  `primaryRepositoryAddress`, `repositoryAddresses`, `repositories`, and
  `legacy`; it has neither local-workspace field nor `cloneUrls`.
- `eventToRepository()` builds those fields at `projectModels.ts:245-286`.
  No `buzz-location` parsing occurs there.
- Repository selection already exists at `projectModels.ts:492-513`: it
  prefers the requested repository, then the project's
  `primaryRepositoryAddress`, then `repositories[0]`.

## 2. `hooks.ts`

No. `hooks.ts` imports `Project` and `Repository` from `./projectModels` at
`desktop/src/features/projects/hooks.ts:58-62` and re-exports them at `:69-75`.
Its legacy-named `eventToProject()` returns `Repository` at `:154-163`; this is
an important naming compatibility wrinkle, not a local type declaration.

## 3. Required search hits

Search scope: `desktop/src` and `desktop/src-tauri` for
`localWorkspacePath`, `localWorkspaceStatus`, `project.cloneUrls`, and
`cloneUrls[`. There are no hits in `desktop/src-tauri`.

### `desktop/src`

- `features/projects/useProjectsRepoSnapshots.ts:41` —
  `cloneUrl: repository.cloneUrls[0] ?? null`; `:50` —
  `const cloneUrl = repository.cloneUrls[0]`. Already repository-correct.
- `features/projects/useProjectCommitDiff.ts:22,28` —
  `project.cloneUrls[0]`; `:54-55` checks
  `!project.localWorkspacePath`/status; `:62-63` includes both local fields in
  the query key; `:70` rejects invalid status.
- `features/projects/ui/useOpenProjectTerminal.ts:33,52` —
  `project.cloneUrls[0]` supplies the terminal clone URL and error context.
- `features/projects/ui/PullRequestReviewCard.tsx:83-84` —
  enables review only when the Project lacks a local workspace and is valid.
- `features/projects/ui/ProjectDetailScreen.tsx:403,592,694` —
  `repository?.cloneUrls[0]` for clone action/error/target. Already
  repository-correct.
- `features/projects/ui/MergePullRequestButton.tsx:128,157` —
  falls back from PR clone URL to `project.cloneUrls[0]`, and depends on
  `project.cloneUrls`.
- `features/projects/repoSyncHooks.ts:31,41,45,69,73,123,127,158,162` —
  all sync/clone/push/pull gating and payloads use `project.cloneUrls[0]`.
- `features/projects/pullRequestMutations.ts:45,72,118,236-237,241,246-247` —
  PR event clone tags, no-clone guard, local-workspace guard, and merge target/
  source all read Project clone/local fields.
- `features/projects/pullRequestMutations.test.mjs:59,83` —
  test assertions use `project.cloneUrls[0]`.
- `features/projects/project-exact-local-workspace-contract.test.mjs:48,78,98,
  114,138-139` — fixtures pass local path/status into the exact-path helper.
  This is helper-input shape, rather than a `Project` read-model assertion.
- `features/projects/project-add-local-workspace-ui-contract.test.mjs:56,64,70,
  75,80-82,93-99` — static assertions explicitly require Project-level
  local-workspace access.
- `features/projects/project-add-local-workspace-read-contract.test.mjs:34-37,
  55-58` — directly asserts that `eventToProject()` has Project-level local
  path/status and clone URLs.
- `features/projects/lib/use-project-merge-recovery-terminal.ts:20` —
  gets `input.project?.cloneUrls[0]`.
- `features/projects/lib/projectsViewHelpers.ts:290` —
  `selectProjectRepository(project, null)?.cloneUrls[0]`; already correct.
- `features/projects/lib/projectLocalRepos.ts:32` —
  `cloneUrlRepoName(project.cloneUrls[0])`.
- `features/projects/lib/project-read-model.ts:20,27-31` —
  creates `localWorkspacePath`, `localWorkspaceStatus`, and a local-path
  dependent `cloneUrls` result.
- `features/projects/lib/project-exact-local-workspace.ts:9,32-34,59-60,66-67,
  72` — helper type and logic carry local path/status (not a Project model).
- `features/projects/hooks.ts:435,456,477,497,665,699` —
  snapshot/diff helpers and query enablement use `project.cloneUrls[0]`.
- `features/projects/branchMutations.ts:22,24,42,44` —
  branch create/delete guard and payload use `project.cloneUrls[0]`.
- `features/home/ui/ProjectInboxDetailPane.tsx:64` —
  `workItem.repository.cloneUrls[0]`; already repository-correct.

## 4. Worktree registry and GitHub target

Current registry command:

- `desktop/src-tauri/src/commands/project_worktree_registry.rs:46-48`:
  `get_project_worktree_registry(repository_path: String)`.
- `:73` calls `fetch_pull_requests_by_branch(&repo_root)`, which derives a GitHub
  target from the checkout.
- `desktop/src/shared/api/agentControl.ts:113-117` exposes the same
  one-argument API.
- `desktop/src/features/agents/projectWorktreeRegistryStore.ts:42-49` caches
  and invokes it by `repositoryPath` only.
- Consumers call that store with a local path: `features/messages/lib/
  useProjectThreadBadge.ts:27-29`, `features/agents/useProjectThreadWorkspace.ts:22-24`,
  and the channel worktree components at `features/channels/ui/
  ChannelWorktreesPill.tsx:20-21` and `ChannelWorktreesDrawer.tsx:47-48`.
  That local path is derived from timeline workspace metadata in
  `features/channels/lib/projectChannelWorkspace.ts:10-13`.

Current GitHub target helper and direct callers:

- `desktop/src-tauri/src/commands/thread_github_target.rs:15-20`:
  `origin_repo_target(repository_path: &Path) -> Option<String>`, always using
  the checkout's `origin`.
- `project_worktree_registry_github.rs:69-82`:
  `fetch_pull_requests_by_branch(repository: &Path)` calls that helper and
  conditionally emits `gh --repo`.
- `thread_github.rs:89-110` calls it for `get_thread_github_status`.
- `thread_workspace.rs:147-186` calls it for `close_thread_pull_request`.

The Phase-2-selected repository must be propagated to the registry API/cache
key and to these GitHub lookups; otherwise both still infer identity from the
checkout remote rather than a Project's selected repository.

## 5. Plan-mentioned contract tests

- `desktop/src/features/projects/project-add-local-workspace-read-contract.test.mjs`
  directly assumes a Project-level local path/status and clone URLs (`:23-60`);
  it must move its assertions to a `Repository` returned by `eventToProject()`.
- `desktop/src/features/projects/project-local-workspace-tag-contract.test.mjs`
  (`:14-161`) validates `buzz-location` tag read/write preservation. It does
  not construct or assert a Project read model, so event fixtures remain valid;
  only terminology should be updated from Project to Repository if desired.

Additional impacted contracts outside the plan's two-file list:
`project-add-local-workspace-ui-contract.test.mjs` (static Project accesses)
and `project-exact-local-workspace-contract.test.mjs` (helper input fixtures).

## 6. Recommended edit order

1. `desktop/src/features/projects/projectModels.ts` — add local workspace
   fields to `Repository`; parse `buzz-location` in `eventToRepository()` using
   Crew's existing validator; retain Project's upstream shape.
2. `docs/crew/DECISIONS.md` — record the minimal upstream-file ownership
   exception and the Repository home for Crew local workspace metadata.
3. `desktop/src/features/projects/lib/project-read-model.ts` and
   `hooks.ts` — retire or adapt the legacy wrapper so it enriches a Repository,
   not a Project; rename local compatibility references where feasible.
4. Replace Project clone/local reads with a selected `Repository` in:
   `useProjectCommitDiff.ts`, `ui/useOpenProjectTerminal.ts`,
   `ui/PullRequestReviewCard.tsx`, `ui/MergePullRequestButton.tsx`,
   `repoSyncHooks.ts`, `pullRequestMutations.ts`, `hooks.ts`,
   `branchMutations.ts`, `lib/use-project-merge-recovery-terminal.ts`, and
   `lib/projectLocalRepos.ts`. Keep the already-correct repository consumers
   unchanged.
5. Update accompanying tests:
   `pullRequestMutations.test.mjs`,
   `project-add-local-workspace-read-contract.test.mjs`,
   `project-add-local-workspace-ui-contract.test.mjs`, and, if helper call
   signatures change, `project-exact-local-workspace-contract.test.mjs`.
6. Propagate selected Repository identity through
   `desktop/src/shared/api/agentControl.ts`,
   `features/agents/projectWorktreeRegistryStore.ts`, the worktree consumers,
   `project_worktree_registry.rs`, `project_worktree_registry_github.rs`, and
   `thread_github_target.rs`; use `primaryRepositoryAddress`, falling back to
   `repositories[0]` for legacy projects.
7. Update Rust callers `thread_github.rs` and `thread_workspace.rs` if the
   target helper receives the explicit repository target rather than deriving
   `origin`.

## Blockers / ambiguities

1. The plan says Rust should receive an explicit `Repository` argument, but Rust
   currently receives only a local filesystem path and has no desktop
   `Repository` model. The implementation must decide the wire shape: a selected
   repository address, selected clone URL, or precomputed `gh --repo` target.
   A Nostr address alone is insufficient for `gh --repo`; the selected
   repository's clone URL or a parsed GitHub target is needed.
2. `eventToProject()` now returns `Repository` despite its name
   (`hooks.ts:154-163`). Updating only its result fields will satisfy runtime
   semantics but leave a misleading compatibility API; deciding whether to
   rename it is scope-sensitive.
3. The plan's validation command requires no `project.cloneUrls` hits. Current
   repository-typed variables are often still named `project`, so type-aware
   refactoring is required instead of blind replacement.

**Status:** DONE_WITH_CONCERNS
**Summary:** The upstream model split has landed; Crew's local-workspace and most clone consumers remain on the old Project surface, while hooks type re-declaration is already removed.
**Concerns/Blockers:** Repository identity needs a defined frontend-to-Rust wire representation for GitHub targeting; a repository address alone cannot be passed to `gh --repo`.
