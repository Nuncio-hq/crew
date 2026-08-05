# Phase 2 — Re-home Crew per-repo surfaces onto upstream's `Repository`

Status: **Ready** · Depends on: Phase 1 · Fixes acceptance criteria 4 and 5

## Problem

Upstream #4671 split the type Crew calls `Project`
(`desktop/src/features/projects/projectModels.ts`):

- `Repository` (kind `30617`) — `cloneUrls`, `dtag`, `defaultBranch`, `repoAddress`
- `Project` (kind `30621`) — `repositoryAddresses[]`, `repositories[]`,
  `primaryRepositoryAddress`, `legacy`

Crew's `Project` (`hooks.ts:69` pre-merge) carries `cloneUrls` and
`localWorkspacePath`. Both belong to upstream's **`Repository`**. Crew's
"Project" is upstream's `Repository`, so these surfaces are semantically correct
under NIP-MP but attached to the wrong type.

## Approach

Re-home, do not rewrite. Ride upstream's compatibility ramp rather than building
one: `repositoryToLegacyProject()` (`projectModels.ts:368`) wraps a bare `30617`
repository into a synthetic project with `legacy: true` and
`repositories: [repository]`, so existing single-repository Crew projects keep
working unchanged.

**Approved decision:** when a project has several repositories, a Crew thread
worktree binds to `primaryRepositoryAddress`.

The `buzz-location` tag stays on kind `30617`. It was always per-repository,
which is exactly what NIP-MP wants — no event-shape change, D-010's mechanism is
untouched.

## Files

Modify:

- `desktop/src/features/projects/hooks.ts` — stop re-declaring `Project`; import
  upstream's `Project` / `Repository` from `./projectModels`.
- `desktop/src/features/projects/lib/projectLocalRepos.ts` — line 32 reads
  `project.cloneUrls[0]`. Caller must select a repository from
  `project.repositories[]` first, then read that repository's `cloneUrls`.
- `desktop/src/features/projects/repoSyncHooks.ts`,
  `useProjectsRepoSnapshots.ts`, `ui/ProjectsView.tsx` — follow the type move.
- `desktop/src-tauri/src/commands/project_worktree_registry.rs` and
  `thread_github_target.rs` — apply the `primaryRepositoryAddress` rule where a
  repository must be chosen.

## Steps

1. Move `localWorkspacePath` / `localWorkspaceStatus` onto `Repository`.
   Upstream owns `projectModels.ts`; adding two Crew fields there is an
   upstream-file edit — record it in `docs/crew/DECISIONS.md` rather than
   creating a parallel Crew type (AGENTS.md § Crew builds on top of Buzz).
2. Replace every `project.cloneUrls` read with a repository selection followed by
   `repository.cloneUrls`.
3. Give the worktree registry and the GitHub target an explicit repository
   argument; default to `primaryRepositoryAddress`, fall back to
   `repositories[0]` for `legacy: true` projects.
4. Update Crew's project contract tests
   (`project-add-local-workspace-read-contract.test.mjs`,
   `project-local-workspace-tag-contract.test.mjs`) for the new type home. The
   tag shape on kind `30617` does not change, so their event fixtures stay valid.

## Validation

- `cd desktop && pnpm lint && pnpm test`
- `cargo test --manifest-path desktop/src-tauri/Cargo.toml`
- `grep -rn 'project\.cloneUrls' desktop/src` returns nothing.
- Manual: an existing single-repository Crew project opens, shows its local
  path, and lists its thread worktrees.

## Risk

`legacy: true` projects and real NIP-MP projects take different paths through
repository selection. A bug here is invisible until someone creates a
multi-repository project. Mitigation: cover both shapes in the contract tests,
not just the legacy one.

## Rollback

Phases 1 and 2 land as one PR. Revert the PR.
