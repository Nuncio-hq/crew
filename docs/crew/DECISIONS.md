# Crew Decisions

This log is append only. When a decision changes, add a new entry that
supersedes the old one; do not rewrite history.

## D-001 — Maintain Crew as a thin Buzz fork

- **Status:** Accepted
- **Date:** 2026-07-30

Crew remains a GitHub fork of `block/buzz`. Prefer new Crew-owned files and
keep edits to upstream files exceptionally small. This preserves the ability
to pull upstream changes and makes maintenance cost visible.

## D-002 — Keep the existing Buzz UI and desktop shell

- **Status:** Accepted
- **Date:** 2026-07-30

Do not restyle the existing Buzz product. New manager-facing UI is
TypeScript/React embedded in the existing Tauri desktop app.

## D-003 — Preserve NIP-34 Project identity

- **Status:** Accepted
- **Date:** 2026-07-30

A repository is identified by `(pubkey, identifier)`. Clone URLs and local
workspace paths are location metadata. Changing a path must not create or
rename a Project.

## D-004 — Keep board state on the relay

- **Status:** Accepted
- **Date:** 2026-07-30

Board cards, columns, assignments, and transitions are signed relay events.
React may project and cache events but is not authoritative. Do not introduce a
separate board database.

## D-005 — Treat the board as an orchestrator

- **Status:** Accepted
- **Date:** 2026-07-30

The columns are `Issues`, `Planned`, `Working`, `Need Input`, and `Done`.
`Working` has a hard cap of three. `Need Input` releases the working slot and
has highest manager priority.

## D-006 — Keep data planes separate

- **Status:** Accepted
- **Date:** 2026-07-30

Coordination events belong on the relay. Source code belongs on the local
filesystem. Large artifacts belong in the media store and are referenced by
URL.

## D-007 — Use subscription-backed agent execution

- **Status:** Accepted
- **Date:** 2026-07-30

Crew uses the user's subscription-backed Codex, Claude Code, Cursor, and other
eligible agent tools. Do not design the normal execution path around metered
API keys.

## D-008 — Require Spike, TDD, then implementation

- **Status:** Accepted
- **Date:** 2026-07-30

Every behavior change begins with a feasibility spike. After a passing spike,
write failing contract tests and design-changing edge cases. Production
implementation begins only after the manager approves the resulting plan.

## D-009 — Do not change ACP session cwd in the first Project slice

- **Status:** Accepted for current slice
- **Date:** 2026-07-30

The first Project implementation stores and surfaces the local location but
does not set `session/new.cwd` to it. The absolute path is delivered through
Project-channel context. A Rust change requires a later spike showing that this
boundary is insufficient and explicit approval.

## D-010 — Make local workspace location native to Buzz Project lifecycle

- **Status:** Accepted
- **Date:** 2026-07-30

The first implementation extends the existing kind `30617` Project
announcement with:

```text
["buzz-location", "local", "<raw absolute path>"]
```

Project creation, location updates, signing, publication, acknowledgement, and
reload continue through Buzz's existing relay lifecycle. The canonical
`buzz-channel` binding is preserved. A local-only Project registry, React-owned
authoritative state, or separate Project database is not an acceptable
fallback when the relay is unavailable.

## D-011 — Keep Git and worktree management out of the local-path slice

- **Status:** Accepted for current slice
- **Date:** 2026-07-30

In this slice, a workspace is an absolute local directory selected by the
manager. Crew does not assert that it is a Git working tree and does not clone,
initialize, discover, validate, create, switch, or remove Git worktrees. An
agent may use Git when the selected directory already supports it, but Crew
does not yet manage or guarantee that behavior. Git integration requires a
separate spike.

## D-012 — Accept the no-Rust picker and restart boundary

- **Status:** Accepted for current slice
- **Date:** 2026-07-30

The official Tauri dialog JavaScript binding may update
`desktop/package.json` and `pnpm-lock.yaml`; these are mechanical dependency
changes outside the two existing behavior-file budget. The existing Rust
plugin registration and capability remain unchanged.

After restart, Crew reads the linked path from the relay but does not claim
that the directory is locally available. Missing or denied paths are reported
when an agent or tool uses them. Proactive restart-time filesystem probing
requires a separate capability spike.

## D-013 — Reuse Buzz release identity for the local NuncioCrew flavor

- **Status:** Accepted for local development
- **Date:** 2026-07-30

The local `NuncioCrew.app` changes the product and display name but retains
bundle identifier `xyz.block.buzz.app` and uses a release build. This allows it
to reuse Buzz's `buzz-desktop` system-Keychain identity, relay configuration,
and app state without exporting or importing a private key.

Buzz and NuncioCrew must not run concurrently because they also share
single-instance scope, app data, deep links, and recovery markers. This
local flavor has only linker ad-hoc signing and is for local use; a separately
identified distributable app requires a new identity-migration, distribution
signing, and notarization decision.

## D-014 — Make Add Project folder-first and relay-authoritative

- **Status:** Accepted for current slice
- **Date:** 2026-07-30

The Projects `+ → Repository` action selects a local folder before any write.
After the manager reviews the exact path and relay destination, Crew creates or
reuses a canonical Project channel and publishes one kind `30617` announcement
containing the normal `(pubkey, d)` identity plus:

```text
["buzz-channel", "<channel-id>"]
["buzz-location", "local", "<raw absolute path>"]
```

The Project is inserted into the UI only after relay acknowledgement and exact
event read-back. Folder-picker cancel is inert. The standalone Local workspace
strip is removed because workspace association is part of Add Project rather
than a separate registration model.

This slice does not inspect `.git`, invent a `clone` tag, clone, initialize Git,
or modify the selected folder. Arbitrary-folder Git detection requires a
separate read-only native-boundary spike.

Spike 0006 completed that investigation and supersedes the assumption that a
new Rust adapter is required: normal non-symlink worktrees can reuse Buzz's
existing read-only snapshot command when TypeScript supplies the selected
path's parent and basename. That read path is not part of D-014's implemented
slice.

## D-015 — Reuse Buzz's reader with exact selected-path isolation

- **Status:** Accepted for current slice
- **Date:** 2026-07-30

For a Project with `buzz-location/local`, Crew derives the selected directory's
parent and basename in TypeScript and passes those values to Buzz's existing
read-only local repository snapshot command. The Project `d` tag is never used
as the filesystem candidate.

Crew accepts the snapshot only when the normalized path returned by the native
command equals the selected workspace path. A linked workspace never falls
back to a same-named checkout under Buzz's configured repositories directory
or to a remote clone. Missing, unreadable, non-Git, symlink-selected, and
mismatched paths are shown as `Local unavailable`.

This reader exposes files, README, commits, contributors, and language data.
It does not enable clone, fetch, pull, push, branch mutation, Terminal,
commit-diff loading, pull-request merge, or agent session cwd for the linked
workspace. Both UI visibility and mutation/query boundaries fail closed.

## D-016 — Separate local, dev, stable, and upstream version identities

- **Status:** Accepted
- **Date:** 2026-07-30

`NuncioCrew Local` remains an ad-hoc local flavor with Buzz bundle identity,
Buzz Keychain service, a visible `Local` marker, and no updater.

Distributed `NuncioCrew` uses bundle identifier `com.nuncio.crew`, Developer ID
signing, Apple notarization, and a Nuncio-owned Tauri updater key. The initial
distribution keeps the existing `buzz-desktop` Keychain service as a migration
boundary; automatic identity migration or full Keychain isolation requires a
later Rust/build-boundary decision.

Crew release tags and the Buzz source baseline are independent:

- Crew dev: `vX.Y.Z-dev[.N]`
- Crew stable: `vX.Y.Z`
- Buzz baseline: version, tag, and exact commit in `upstream-buzz.json`

Releases run only through the manager-triggered GitHub workflow. Dev releases
advance only the dev updater manifest. Stable releases advance stable and dev,
allowing dev installations to graduate. Stable installations never receive a
dev prerelease.

Release credentials live in a `main`-only GitHub Environment rather than
repository-wide secrets. All versions share one release queue, rolling
manifests move forward only, and a versioned release is made public before a
channel points at its immutable updater archive. CI also proves the updater
signature key ID matches the public key embedded in the app.

The first `v0.0.1-dev` installation is manual because the existing local build
reports Buzz version `0.5.2` and an updater must never downgrade it.

## D-017 — Require one macOS-first Crew merge gate

- **Status:** Accepted
- **Date:** 2026-07-30

Normal Crew pull requests require exactly one stable status,
`NuncioCrew Gate`. It composes a fast desktop check, an unsigned macOS Apple
Silicon package, and a real-relay Project contract only when its relevant paths
change.

Web, mobile, Windows, Linux distribution, Docker publication, Kubernetes,
Sprig publication, and optional mesh-llm native builds are not automatic merge
requirements for the current one-manager product. Full Rust compatibility
is not claimed; a manual upstream-sync workflow retains the core Rust format,
lint, unit, and dependency-policy checks for both root and desktop Tauri
workspaces.

Buzz workflow source files remain unchanged for upstream synchronization.
Inherited automatic workflows are disabled in GitHub repository state only
after the additive Crew gate passes, and can be re-enabled as rollback.
