# Spike 0005 — Folder-first Project creation

- **Status:** PASS WITH LIMITATION
- **Date:** 2026-07-30

> **Superseded evidence:** Spike 0006 later proved that the existing Rust Git
> reader can inspect a selected real directory when TypeScript supplies its
> parent and basename. The no-Rust Git limitation recorded below was
> conservative; no new Rust command is required for normal non-symlink paths.

## Question

Can NuncioCrew make `+ → Repository` behave like opening a local project in
Codex or Cursor while still creating the canonical NIP-34 Project on Buzz's
relay, without changing Rust?

## Decision affected

Whether Add Project can be a thin TypeScript feature or needs a new native
filesystem boundary.

## Hypothesis

The existing Tauri folder dialog and relay signing APIs are enough to select a
folder and publish kind `30617` with local-path and canonical-channel metadata.

## Scope

- Select one existing local directory.
- Create a Project channel.
- Publish and read back one signed kind `30617` Project announcement.
- Preserve the selected path as location metadata, never Project identity.

## Exclusions

- Reading `.git` or a Git remote from an arbitrary selected folder.
- Adding a `clone` tag.
- Clone, checkout, `git init`, or any write inside the selected folder.
- Making `session/new.cwd` use the selected path.

## Evidence

- `@tauri-apps/plugin-dialog` is already registered and can select a directory.
- The desktop frontend has no filesystem plugin or filesystem capability.
- Existing Project Git commands resolve repositories only below configured
  Buzz `REPOS` roots; they do not inspect an arbitrary selected folder.
- Existing relay code can sign, publish, await acknowledgement, and read back a
  kind `30617` event.

## Pass criteria

- Folder selection is the first Add Project action.
- Cancel has no channel or relay side effect.
- The folder basename provides the default Project name.
- The event contains `d`, `name`, one `buzz-channel`, and exactly one
  `["buzz-location", "local", rawPath]` tag.
- No `clone` tag is fabricated.
- Success is shown only after relay acknowledgement and exact read-back.
- The selected folder is not modified.

## Verdict

PASS WITH LIMITATION: folder-first, relay-native Project creation is feasible
in TypeScript with the existing Tauri dialog. Git detection is not feasible for
an arbitrary folder under the current no-Rust boundary, so this slice must omit
Git metadata rather than guess it.

## Follow-up

See
[`0006-reuse-existing-git-reader-for-exact-local-workspace.md`](0006-reuse-existing-git-reader-for-exact-local-workspace.md).
Reuse the existing read-only command before considering any new Rust adapter.
