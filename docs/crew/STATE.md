# Crew State

Last updated: 2026-07-30

## Repository

- GitHub: `https://github.com/Nuncio-hq/crew`
- Fork parent: `https://github.com/block/buzz`
- Default branch: `main`
- Baseline upstream commit: `63496cc1d4c6f1b7c613801bdcc694169dcf391a`
- Production code changes: implemented on a feature branch; first PR pending

## Current product slice

Make a Buzz Project record point to a local workspace directory while
preserving NIP-34 identity.

In scope:

- local workspace as Project location metadata;
- the approved `buzz-location/local/raw-path` record;
- Project create and update through the existing kind `30617` relay lifecycle;
- folder-first `+ → Repository` creation in the Projects page;
- canonical `buzz-channel` binding and relay acknowledgement;
- Project-channel context containing the absolute path;
- one-machine, one-manager use;
- provider compatibility through existing ACP paths.

Out of scope for this slice:

- changing `session/new.cwd`;
- a per-Project Rust dispatcher;
- Git repository or worktree management;
- clone, init, fetch, pull, push, branch, or remote validation;
- commit-diff loading for an exact linked workspace;
- board implementation;
- mobile;
- multi-user local-path sharing;
- final mention model syntax.
- Windows drive and UNC workspace paths.

## Local desktop build

- Additive flavor name: `NuncioCrew Local`.
- Artifact:
  `desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/NuncioCrew Local.app`.
- Build profile: Apple Silicon release with linker ad-hoc signing; no
  distribution identity or notarization.
- Bundle identifier: `xyz.block.buzz.app`.
- Identity store: existing system-Keychain service `buzz-desktop`.
- Buzz and NuncioCrew must not run concurrently.
- The build includes real release versions of all five agent sidecars.
- Settings displays `v0.5.2 · Local`.
- Updater configuration and updater signing are disabled for this flavor.

## Release lane

- GitHub workflow: `.github/workflows/nuncio-crew-release.yml`.
- Trigger: manual `workflow_dispatch` only.
- First planned release: `v0.0.1-dev`.
- Initial platform: macOS Apple Silicon.
- Distributed identity: `com.nuncio.crew`.
- Dev manifest: `nuncio-crew-dev-latest/latest.json`.
- Stable manifest: `nuncio-crew-stable-latest/latest.json`.
- Signing secrets: protected `nuncio-crew-release` GitHub Environment, `main`
  branch only.
- Safety: one global release queue, current-main-only source, monotonic rolling
  manifests, public versioned assets before channel advance, updater key-ID
  match, and explicit entitlements verification.
- Buzz source pin: `upstream-buzz.json`, currently `0.5.2` at
  `63496cc1d4c6f1b7c613801bdcc694169dcf391a`.
- The protected Environment, reviewer, nine encrypted release secrets, updater
  public variable, and Nuncio updater keypair are configured.
- Release publication remains pending PR merge, manager-approved signed dry
  run, clean-install verification, and a later explicit publish run.

## Verified evidence

- `buzz-acp` currently captures one process cwd for its prompt context.
- Project announcements already support `buzz-channel` binding.
- `buzz-dev-mcp` accepts absolute paths and shell `workdir`.
- Codex, Claude Code, Cursor, and Devin all completed an isolated absolute
  Project-path read/write probe while session cwd remained elsewhere.
- Codex required the Buzz MCP path for the external write; its native
  workspace-write path was blocked.
- Spike 0002 selected a Crew extension record containing a raw absolute path:
  `["buzz-location", "local", "<absolute-path>"]`.
- Spike 0003 proved the official Tauri directory picker in a real `Buzz.app`;
  cancel, Unicode paths, spaces, and relink passed without a Rust edit.
- A Postgres-backed Buzz relay test published kind `30617`, linked a path,
  reconnected for a cold read, relinked a Unicode path, and resolved the latest
  path into explicit-agent context.
- The selected location record preserves NIP-34 identity and clone semantics;
  Buzz stores and preserves unknown metadata tags.
- The manager confirmed that the relay lifecycle is mandatory: a filesystem
  path never replaces Buzz Project registration or relay authority.
- A local workspace is not required to be a Git worktree in this slice.
- Add Project now selects a folder first, derives an editable default name,
  creates/reuses a Project channel, and publishes the Project only after
  explicit plaintext-path consent.
- The standalone Local workspace strip has been removed from the Projects page.
- The Project event does not fabricate a `clone` tag.
- Spike 0006 proved that normal selected Git worktrees can be read without a
  Rust change by supplying `dirname(localWorkspacePath)` and
  `basename(localWorkspacePath)` to Buzz's existing local snapshot command.
- The exact reader is now implemented for Project detail and overview. It
  requires the snapshot path returned by Rust to equal the selected workspace,
  never falls back to a same-named Buzz checkout or remote clone, and exposes
  files, README, commits, contributors, and language data read-only.
- Symlink-selected, missing, unreadable, and non-Git workspaces remain
  unavailable under the existing containment and repository checks.

See [`spikes/0001-project-workspace-absolute-path.md`](spikes/0001-project-workspace-absolute-path.md).
See [`spikes/0002-project-local-location-schema.md`](spikes/0002-project-local-location-schema.md).
See [`spikes/0005-folder-first-project-create.md`](spikes/0005-folder-first-project-create.md),
[`spikes/0006-reuse-existing-git-reader-for-exact-local-workspace.md`](spikes/0006-reuse-existing-git-reader-for-exact-local-workspace.md),
[`verification/0003-folder-first-add-project.md`](verification/0003-folder-first-add-project.md),
[`verification/0004-exact-local-workspace-reader.md`](verification/0004-exact-local-workspace-reader.md),
and the
[`exact-reader plan`](../../plans/20260730-1535-exact-local-workspace-git-reader/plan.md).

## Current gate

Exact local workspace reading is implemented and its automated gates pass.
Computer Use opened the rebuilt release and reached Projects, but the current
relay returned no Projects. Exact repository data could not be smoked without
publishing a new real relay event, which was intentionally not done.

## Current test gate

- Thirteen additive Project workspace test files cover parsing, duplicate
  locations, metadata preservation, NIP-01 replacement ordering, owner
  protection, relay rejection/read-back, privacy copy, Project-channel
  matching, fresh context, no stale fallback, consent readiness, retry channel
  reuse and ACK recovery, folder-first creation, read-side local metadata,
  clone suppression, malformed-metadata fail-closed behavior, configured
  checkout collision isolation, empty-state create access, Markdown isolation,
  live relay reconstruction, exact local path resolution, mismatch rejection,
  no fallback, and truthful Local source state.
- Latest full desktop suite: `3857` passed, `1` gated live-relay test skipped,
  zero failed.
- Earlier focused live relay test: `1/1` passed with an isolated Buzz relay.
- Typecheck, file-size gate, Biome checks, production build, and
  `git diff --check` passed.
- No release tag or public artifact has been created.
- Manual release contracts: `10/10` passed.
- Real unsigned Tauri bundle spike accepted `0.0.1-dev` and produced
  `NuncioCrew.app` with identifier `com.nuncio.crew`.

## Open decisions

- Whether a future non-local relay must hard-block local-path publication or
  use a different privacy mechanism.
- Final board event kind and tag schema.
- Whether exact local snapshots should additionally refresh on app focus.
- Whether symlink-selected workspaces should remain unsupported or get a
  separately reviewed canonical-path flow.
- When to publish or link a real Project on the manager relay for the final
  native exact-reader smoke.
