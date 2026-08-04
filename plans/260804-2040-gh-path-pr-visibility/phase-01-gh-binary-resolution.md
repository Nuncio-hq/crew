# Phase 1 — Resolve the `gh` binary instead of trusting PATH

Status: **Ready** · Depends on: — · Fixes acceptance criteria 1 and 2

## Problem

Five Tauri commands spawn `gh` by bare name. A macOS app launched from Finder
gets launchd's PATH (`/usr/bin:/bin:/usr/sbin:/sbin`), which does not contain
Homebrew's `/opt/homebrew/bin`. Every `gh` call in the packaged app fails.

Evidence and the full call-site table are in [`README.md`](README.md#root-cause-verified-2026-08-04-at-14dc86bb1).

## Approach

One shared helper that resolves `gh` once and hands back a ready-to-spawn
`tokio::process::Command` with an augmented `PATH` in its environment. The five
call sites switch to it. No PATH list is written by hand — the helper delegates
to `crate::managed_agents::find_command`, which is the app's existing binary
resolution policy.

`PATH` is set on the child in addition to using the absolute binary path,
because `gh` itself shells out to `git` for remote resolution.

## Files

Create:

- `desktop/src-tauri/src/commands/gh_cli.rs` — resolver + command builder.

Modify:

- `desktop/src-tauri/src/commands/mod.rs` — register the module.
- `desktop/src-tauri/src/commands/thread_github.rs` — lines 118, 141.
- `desktop/src-tauri/src/commands/project_worktree_registry_github.rs` — line 60.
- `desktop/src-tauri/src/commands/thread_workspace.rs` — lines 157, 175.

## Steps

1. **Confirm reachability before writing the helper.** `find_command` is
   `pub(crate)` in `managed_agents/discovery.rs:999`, re-exported by
   `pub use discovery::*` (`managed_agents/mod.rs:50`), but has no caller
   outside `discovery.rs` today. Write a throwaway call from `commands/` and
   compile. If visibility does not reach, widen the re-export in
   `managed_agents/mod.rs` rather than duplicating the search logic.

2. **Write `gh_cli.rs`.** Shape:

   ```rust
   /// Why a `gh` invocation could not be attempted.
   pub(crate) enum GhUnavailable {
       /// No `gh` on any known path.
       CliMissing,
   }

   /// Resolved absolute path to `gh`, cached across calls.
   ///
   /// Only successful resolutions are cached: a user who installs `gh` while
   /// the app is running should not have to restart. Repeated misses stay
   /// cheap because the login-shell PATH probe behind `find_command` has its
   /// own cache.
   async fn gh_binary() -> Option<PathBuf>;

   /// A `gh` command with the resolved binary and an augmented `PATH`, so the
   /// `git` subprocesses `gh` spawns resolve too.
   pub(crate) async fn gh_command() -> Result<Command, GhUnavailable>;
   ```

   `find_command` is synchronous and its first call may spawn a login shell, so
   `gh_binary` must run it inside `tokio::task::spawn_blocking`. Cache with a
   `OnceLock<PathBuf>`-style store written only on success.

   Augmented PATH comes from
   `crate::managed_agents::readiness::cli_probe::augmented_path()`
   (`managed_agents/readiness/cli_probe.rs:9`) — the same source
   `probe_auth_status` uses (`discovery.rs:1034`).

3. **Switch `thread_github.rs`.** `find_pull_request_number` and
   `read_pull_request` both become `Option`-returning as today; a
   `GhUnavailable` maps to `None`, preserving current control flow. Phase 2
   changes what that `None` means to the UI — keep this phase behaviour-neutral
   apart from the resolution fix.

4. **Switch `project_worktree_registry_github.rs`.**
   `fetch_pull_requests_by_branch` already returns `Option` and its caller
   (`project_worktree_registry.rs:70`) already maps `None` to
   `github: unavailable`. Only the spawn changes.

5. **Switch `thread_workspace.rs`.** `close_thread_pull_request` returns
   `Result<_, String>`, so an unresolvable `gh` should surface a real message —
   `"GitHub CLI (gh) was not found."` — not a generic
   `"Could not start command: ..."`. This is a user-triggered destructive
   action; a vague error here is worse than elsewhere.

6. **Do not touch the `git` call sites** in `thread_workspace_git.rs`.
   `/usr/bin/git` is on the default PATH; changing them is out of scope and
   would widen the blast radius of a bug fix. Noted as a follow-up: a Mac
   without Xcode CLT has a `/usr/bin/git` stub that prompts instead of running.

## Tests

- Unit, in `gh_cli.rs`: resolution returns the absolute path when the binary
  exists on a temp dir injected into PATH, and `CliMissing` under an emptied
  PATH. Follow the PATH-swapping pattern already used in
  `managed_agents/readiness.rs:1268-1285`.
- Unit: the built command carries an augmented `PATH` env entry.
- `cargo test --manifest-path desktop/src-tauri/Cargo.toml` — the desktop crate
  is excluded from the root workspace, so a root `cargo test` does not cover it.

## Validation

```bash
just ci
cargo test --manifest-path desktop/src-tauri/Cargo.toml
```

Then the GUI-PATH repro from [`README.md`](README.md#verification): launch the
built app under `env -i PATH=/usr/bin:/bin:/usr/sbin:/sbin`, open a project
thread that has an open PR, and confirm the PR and CI chips render. Confirm the
channel header pill reads `N worktrees · M PRs open`.

Attribute the result to the commit it ran at — `git rev-parse HEAD` in the same
shell — before claiming the fix holds.

## Risk and rollback

Low. The change is confined to how a subprocess is located; the `gh`
invocations, their arguments, and every downstream parse are untouched. If the
resolver misbehaves, reverting `gh_cli.rs` and the five call sites restores
exactly today's behaviour.

The one new failure mode is a slow first resolution if the login-shell probe
hangs. `find_command`'s probe already runs under the app's existing timeout
policy and its result is cached, and the `gh` calls themselves sit behind a
30s frontend cache (`projectThreadGitHubStore.ts:19`).
