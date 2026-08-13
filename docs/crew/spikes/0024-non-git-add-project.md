# Spike 0024 — Non-git folder at Add Project (#187)

- **Status:** PASS
- **Date:** 2026-08-13
- **Issue:** [#187](https://github.com/Nuncio-hq/crew/issues/187)

## Question

What does the current Add Project flow do with a non-git folder: accept it
and break on the first agent turn, or refuse at the boundary?

## Decision affected

D-053 / issue #187 — git-ness gate at add-Project. The Cowork (non-git)
product is [#188](https://github.com/Nuncio-hq/crew/issues/188); this spike
only records today's broken path so the gate can close it honestly.

## Hypothesis

Add Project accepts any absolute folder. The confirm dialog even says Crew
will not initialize Git. The first agent turn then fails inside
`plan_thread_worktree` at `git rev-parse --show-toplevel`.

## Scope

- Providers: Crew desktop Add Project UI + ACP `plan_thread_worktree`
- Files:
  - `desktop/src/features/projects/lib/project-local-workspace.ts`
    (`validateLocalWorkspacePath`)
  - `desktop/src/features/projects/ui/crew-add-project-flow.tsx` (pre-#187)
  - `desktop/src/features/projects/ui/crew-add-project-dialog.tsx`
  - `crates/buzz-acp/src/thread_workspace.rs` (`plan_thread_worktree`)
- Time: one source-path inspection; no live folder picker in this environment

## Exclusions

- Implementing Cowork mode (#188)
- Changing git identity of an existing Project
- Auto-init of a `.git` directory

## Pass criteria

A written record of the pre-fix path: accept vs refuse, and where the first
agent turn fails, with file citations.

## Fail criteria

Claiming the add-flow already refused non-git folders without a git probe.

## Environment

- Commit: Crew `main` after #191 (`0280810b2`, Buzz 0.5.11 sync)
- OS: Linux cloud agent
- Tool/provider versions: in-repo source only
- Authentication class: none

## Method

1. Read `validateLocalWorkspacePath` — absolute path, no NUL/CR/LF; no
   `rev-parse`.
2. Read pre-#187 `CrewAddProjectFlow.chooseFolder` — folder picker then
   confirm dialog; no git probe.
3. Read confirm copy in `crew-add-project-dialog.tsx`.
4. Read `plan_thread_worktree` first git call.

## Results

| Step | Pre-#187 behavior | Evidence |
| --- | --- | --- |
| Path validation | Absolute path only | `validateLocalWorkspacePath` throws `"Choose an absolute local folder path."` for relative / empty / NUL paths. `/` is accepted. No `git` invocation. |
| Add flow | Accepts the folder | `chooseFolder` opened the confirm dialog after `chooseProjectWorkspaceFolder()` returned a path. |
| Confirm copy | Explicitly not git-init | `"The folder stays where it is. NuncioCrew will not clone, initialize Git, or change its files."` |
| First agent turn | Provisioning error | `plan_thread_worktree` canonicalizes the path then runs `git rev-parse --show-toplevel`. A non-git folder fails here as an ACP protocol error, not a named "no workspace" mode. |

## Edge cases observed

- A folder that is *inside* a git worktree (`show-toplevel` walks up) would
  have been accepted and bound to the ancestor repo — surprising, but still
  git. Out of scope for the refuse-at-add gate.
- The confirm dialog's "will not initialize Git" copy remains accurate after
  the gate: git folders are used in place; non-git folders never reach it.

## Limitations

- This environment did not click the live folder picker. The verdict is from
  the source path the UI and harness actually run.

## Verdict

**PASS.** Pre-#187 Add Project accepted non-git folders and the harness
errored on turn one. The #187 fix refuses at add-Project with copy pointing
at #188.

## Follow-up test contract

- Tauri `probe_project_git_workspace` returns `isGit: false` for a folder
  with no `.git` (unit test in `project_git_workspace_probe.rs`).
- `CrewAddProjectFlow.chooseFolder` toasts `NON_GIT_PROJECT_REFUSAL` and does
  not open the confirm dialog when `isGit` is false.

## Cleanup

No temporary data. Source-only spike.
