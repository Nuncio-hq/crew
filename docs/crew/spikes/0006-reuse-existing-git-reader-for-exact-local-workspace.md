# Spike 0006 — Reuse the existing Git reader for an exact local workspace

- **Status:** PASS WITH LIMITATION
- **Date:** 2026-07-30

## Question

Can NuncioCrew make a folder-first Project immediately readable as `Local`
without adding or changing a Rust command?

## Hypothesis

For a selected path such as `/Users/oscar/Projects/crew`, the frontend can pass
`/Users/oscar/Projects` as `reposDir` and `crew` as the repository candidate to
Buzz's existing `get_project_local_repo_snapshot` command.

## Evidence

- `canonical_repos_roots` accepts an explicit absolute, accessible directory;
  it is not restricted to the persisted community `reposDir`.
- `find_local_repo_dir` joins that root with a candidate, canonicalizes it,
  requires it to remain below the root, and then checks for `.git`.
- `get_project_local_repo_snapshot` returns the resolved path plus a read-only
  snapshot of commits, files, and contributors.
- `@tauri-apps/api/path` already exports asynchronous `dirname` and `basename`
  helpers, so the frontend does not need to parse platform paths itself.
- The real selected repository
  `/Users/a1241968/Desktop/Oscar/LilGroup/Nuncio/crew` has a `.git` directory.

## Empirical test

Temporary tests were added to the real Rust modules, executed, and then removed.
No Rust or Cargo diff remains.

```text
running 4 tests
...unicode_and_spaces ... ok
...project_dtag_differs ... ok
...symlink_that_escapes_the_parent ... ok
...reads_the_selected_real_repository_snapshot ... ok

test result: ok. 4 passed; 0 failed
```

The real-repository test resolved `dirname(selected) + basename(selected)` back
to the exact canonical `crew` path, then read non-empty latest-commit, commits,
and files data through `snapshot_from_worktree`.

## Edge results

- Spaces and Unicode folder names: pass.
- Editable Project name or d-tag differing from the folder name: pass only when
  the caller supplies the folder basename as the resolver candidate; using the
  Project d-tag fails as expected.
- Same-named symlink escaping the supplied parent: rejected by the existing
  containment guard.
- Folder without `.git`: returns no local snapshot.
- No clone URL or network access is required for the local snapshot.

## Verdict

PASS WITH LIMITATION. A new Rust adapter is not required for normal real
directories. The existing command can read the exact selected worktree when
TypeScript derives and supplies the selected path's parent and basename.

Symlink-selected workspaces remain unsupported by this reuse path because the
resolver intentionally rejects a candidate whose canonical target escapes its
supplied parent. The UI should report that clearly rather than fall back to a
same-named configured checkout.

## Smallest implementation

1. Add a TypeScript helper using Tauri `dirname` and `basename`.
2. For a Project with `localWorkspacePath`, call the existing local snapshot
   command with the derived parent and basename, not Project d-tag.
3. Accept the result only when its returned canonical path matches the selected
   workspace path; otherwise fail closed.
4. Keep remote clone, fetch, pull, push, and Git mutation out of this slice.
5. Replace `Local missing` with a truthful linked/checking/unavailable state.
