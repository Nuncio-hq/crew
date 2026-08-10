# Crew State

Last updated: 2026-08-10

## Founder product direction (docs)

Locked narrative for agents (not a shipped feature checklist):

- [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md) — company-on-machine, Hermes-first
  on Buzz contracts, mobile continuity, in/out scope
- [`AGENT-WORKING-AGREEMENT.md`](AGENT-WORKING-AGREEMENT.md) — plain language,
  honesty, no assumed manager experience
- Decisions **D-025**, **D-026**, **D-027** in [`DECISIONS.md`](DECISIONS.md)
  (upstream **D-024** remains Hermes trusted/owner-only/local)

Implementation slices below remain the code truth for what is built today.

## Repository

- GitHub: `https://github.com/Nuncio-hq/crew`
- Fork parent: `https://github.com/block/buzz`
- Default branch: `main`
- Baseline upstream commit: `63496cc1d4c6f1b7c613801bdcc694169dcf391a`
- Production code changes: merged through PR #1 and PR #2
- Required merge gate: additive `NuncioCrew CI`, macOS Apple Silicon only;
  `NuncioCrew Gate` is enforced on `main`

## Current product slice

Evidence on the thread log is shipped for this slice:

- the CLI appends `["crew-evidence", "<kind>"]` for the four validated evidence
  kinds to existing messages;
- desktop renders Crew evidence cards for kind 9, preserves ordinary fallback,
  and keeps kind 46043 receipt cards unchanged;
- owners can Accept/Reject with existing kind-7 reactions, with Reject opening
  the normal reply composer.
- C2/C3 Playwright contracts and the desktop unit suite verify these behaviors.

Make a Buzz Project record point to a local workspace directory while
preserving NIP-34 identity.

In scope:

- local workspace as Project location metadata;
- the approved `buzz-location/local/raw-path` record;
- Project create and update through the existing kind `30617` relay lifecycle;
- folder-first `+ → Repository` creation in the Projects page;
- canonical `buzz-channel` binding and relay acknowledgement;
- Project-channel context containing the absolute source path;
- per-thread ACP scheduling and isolated Git worktree cwd;
- owner-scoped worktree-ready/error telemetry and a root-scoped Project-thread
  workspace surface;
- office-level completion evidence guidance and the `messages send
  --evidence <kind>` tag surface;
- desktop evidence cards with owner Accept/Reject reactions on ordinary kind-9
  messages;
- ordered Project-thread handoff state from mentions, active-turn telemetry,
  and signed agent replies;
- ordered multi-agent Project task routing through normal composer mentions;
- one-machine, one-manager use;
- provider compatibility through existing ACP paths.

Out of scope for this slice:

- a per-Project Rust dispatcher outside the ACP harness;
- clone, init, fetch, pull, push, branch, or remote validation;
- commit-diff loading for an exact linked workspace;
- board implementation;
- mobile;
- multi-user local-path sharing;
- automatic semantic branch renaming after an agent proposes a human title;
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
- Settings displays the pinned Buzz version `v0.5.7 · Local`; the
  machine-readable source is [`upstream-buzz.json`](upstream-buzz.json).
- Updater configuration and updater signing are disabled for this flavor.

## Release lane

- GitHub workflow: `.github/workflows/nuncio-crew-release.yml`.
- Trigger: manual `workflow_dispatch` only.
- First published release: `v0.0.1-dev`.
- Initial platform: macOS Apple Silicon.
- Distributed identity: `com.nuncio.crew`.
- Dev manifest: `nuncio-crew-dev-latest/latest.json`.
- Stable manifest: `nuncio-crew-stable-latest/latest.json`.
- Workflow versions stay semantic (`v0.0.5` or `v0.0.5-dev.N`); immutable
  releases use Crew-owned tags (`crew-v0.0.5` or `crew-v0.0.5-dev.N`) while
  artifact and manifest versions omit both prefixes.
- Release collision checks and archive URLs use the Crew-owned immutable tag,
  so inherited unprefixed Buzz tags cannot block Crew publication.
- Signing secrets: protected `nuncio-crew-release` GitHub Environment, `main`
  branch only.
- Safety: one global release queue, current-main-only source, monotonic rolling
  manifests, public versioned assets before channel advance, updater key-ID
  match, and explicit entitlements verification.
- Buzz source pin: [`upstream-buzz.json`](upstream-buzz.json), currently
  `0.5.7` / `desktop-v0.5.7` at
  `f167818d25dd9f03115ab907a16f07daee2ece5c`.
- The protected Environment, reviewer, nine encrypted release secrets, updater
  public variable, and Nuncio updater keypair are configured.
- Signed dry run `30537460233` and publish run `30538712572` passed.
- The public DMG is signed, notarized, stapled, ARM64-only, and launch-tested
  from the mounted image. Real-profile relay and Project acceptance remains a
  manager test after manual installation.

## CI lane

- Required merge signal: `NuncioCrew Gate`.
- Automatic checks: desktop fast gate, unsigned macOS ARM64 package, and a
  path-filtered real-relay Project contract.
- Web, mobile, Windows, Linux distribution, Docker publishing, Helm, Sprig,
  and optional mesh-llm builds are outside automatic Crew CI.
- Core root and desktop Tauri Rust format, lint, unit, and dependency-policy
  checks are available through manual `NuncioCrew Upstream Sync`; it does not
  claim full platform or integration compatibility.
- Inherited Buzz workflow files remain unchanged and are disabled only through
  GitHub repository state after the Crew gate is proven.

## Verified evidence

- Evidence-card C2 and owner-reaction C3 Playwright contracts pass, including
  full authored text excerpts for the text layouts and owner-consistent verdict
  state.
- Phase 09 live probe results are recorded in
  [`verification/0010-evidence-on-thread-log-probes.md`](verification/0010-evidence-on-thread-log-probes.md).
  Probe 1 is explicitly split between the mock desktop command-payload check
  and the separate real-relay reaction read-back; the single-process
  click-to-relay chain remains unverified; issue #133 tracks the relay-backed
  `add_reaction` bridge follow-up.
- The desktop unit suite passes with 5045 tests passing, one skipped, and zero
  failures.
- `buzz-acp` uses the process cwd for ordinary sessions and one validated,
  deterministic worktree cwd for each owner-authored Project task thread.
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
- Thread worktrees record their full root event ID in branch-local Git config.
  Existing `0.0.4` worktrees without that record are adopted only after their
  canonical path, common Git directory, and deterministic branch all verify;
  conflicting roots fail closed.
- The ACP harness emits owner-scoped encrypted `thread_workspace_ready` and
  `thread_workspace_error` frames. Ready includes the verified path, worktree
  name, branch, and base revision; errors expose only the root ID and a safe
  message.
- Desktop keeps a bounded workspace projection by thread root, preserves each
  community's latest projection across community switches, and rejects stale
  observer frames. Project threads show preparing, ready, or error state, a
  branch/details popover with copy-path action, and an ordered handoff list.
  Working comes from conversation-scoped active turns; done requires a signed
  agent reply.
- Ordinary channels and non-Project threads keep the existing UI and composer.
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

Releases are published through [`crew-v0.0.9`](https://github.com/Nuncio-hq/crew/releases/tag/crew-v0.0.9),
released 2026-08-07, and it is the latest release. The `0.0.6`
thread-worktree line merged and was released; it is not an in-flight
candidate. No signed updater install and relaunch has been verified on a
release pair in the repository evidence yet, so that remains a required
release verification. Worktree freshness is measured from the thread
worktree's actual `HEAD`; an unavailable fetch reports an unknown remote
distance, and lifecycle actions require both the live branch ownership record
and its durable root claim.

Attention/recovery work is merged through #108 (`6793c86da`), #113
(`304173e42`), and #114 (`35af74019`, the current `origin/main` head). The
roles track is issue #116, with PR #120 (`feat/issue-116-agent-roles`) open
and in flight.

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
- Latest full desktop suite: `3873` passed, `1` gated live-relay test skipped,
  zero failed.
- Full `just ci` passed on the thread-worktree orchestration branch, including
  Rust workspace tests, `1905` native desktop tests (`14` ignored), `906`
  mobile tests (`1` skipped), frontend builds, lint, typecheck, and formatting.
- Focused browser verification passed `62` Project composer, mention,
  messaging, thread-anchor, and boot-flow scenarios against the E2E bridge.
- Thread-worktree verification passed `14` focused provisioning tests, `5`
  session-cwd tests, all `661` `buzz-acp` library tests plus `9` integration
  tests, and `18` focused Desktop/release-contract tests. Three focused native
  lifecycle tests also cover dirty-worktree refusal, clean removal, and safe
  rejection of a later branch that reuses a stale deterministic name.
- The full Desktop suite passed `3885` tests with one environment-gated live
  relay test skipped; Project-thread Playwright verification passed after a
  fresh E2E build and showed distinct workspace state across two roots. Its two
  scenarios also cover the explicit failed-workspace UI.
- Exact observer controls preserve `conversationId` and `turnId`; concurrent
  same-channel turns can be stopped independently, and unconfirmed model
  switches no longer surface as successful.
- The local macOS arm64 bundle was built and ad-hoc signed at
  `desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/NuncioCrew Local.app`.
- Earlier focused live relay test: `1/1` passed with an isolated Buzz relay.
- Typecheck, file-size gate, Biome checks, production build, and
  `git diff --check` passed.
- The latest published Crew release is `crew-v0.0.9` (2026-08-07); the
  `crew-v0.0.6` thread-worktree release is part of that published history.
- Manual release contracts: `10/10` passed.
- Always-run Crew CI/local/release contracts: `20/20` passed.
- Real unsigned Tauri bundle spike accepted `0.0.1-dev` and produced
  `NuncioCrew.app` with identifier `com.nuncio.crew`.

## Open decisions

- **Resolved by D-037(2):** Final board event kind and tag schema is deferred;
  no board event kind or tag schema is defined until a board-like surface is
  prioritized.
- **Decided: yes; task:** Exact local snapshots refresh on app focus with a
  debounced point-in-time re-read. File-watcher liveness is a future upgrade
  tied to the work overview lens in D-037(3). The small implementation task is
  filed as #139.
- **Decided: stay unsupported:** Symlink-selected workspaces fail closed as
  shipped. GitHub identity and the real project path are the source of truth;
  revisit only on a concrete user need.
- **Deferred; decided-with: mobile epic:** Non-local relay local-path privacy
  is a mandatory spike question for the future mobile-continuity epic (D-026).
- **Converted to task:** Publishing or linking a real Project on the manager
  relay for the native exact-reader smoke belongs in the next
  release-verification Definition of Done, alongside the outstanding signed
  updater install/relaunch verification. It is not an open decision.

- Phase 07's generic upstream contribution remains a draft only under D-020;
  no upstream PR is open.

- Whether a future non-local relay must hard-block local-path publication or
  use a different privacy mechanism.
- Final board event kind and tag schema.
- Whether exact local snapshots should additionally refresh on app focus.
- Whether symlink-selected workspaces should remain unsupported or get a
  separately reviewed canonical-path flow.
- When to publish or link a real Project on the manager relay for the final
  native exact-reader smoke.

## Hermes runtime track (feature 0001)

- Feature plan:
  [`features/0001-hermes-first-class-runtime.md`](features/0001-hermes-first-class-runtime.md);
  decisions locked as D-019; runbook [`HERMES.md`](HERMES.md).
- Slice 0 (five spikes, records 0009–0013) complete: profile-bound spawn,
  headless lifecycle, and bounded concurrency PASS; no headless auth probe
  exists in Hermes v0.20.0 (ask filed); `BUZZ_ACP_MODEL` leak paths and
  suppression point identified.
- Slice 1 verified live
  ([`verification/0006`](verification/0006-hermes-slice1-live-roundtrip.md)):
  mention → profile-bound Hermes turn → signed `CREW-LIVE-OK` reply over a
  real relay, and a profile-side model change picked up by the running
  adapter after `!rotate` with no respawn. Operational requirement found:
  `BUZZ_ACP_MCP_COMMAND=buzz-dev-mcp` is mandatory (Hermes' sandbox strips
  `BUZZ_*` from its own terminal tool).
- Next gates: Slice 2 (binding/readiness/no-model UI + RED contracts),
  Slice 3 (upstream tier-1 PR to block/buzz), Slice 4 (profile lifecycle
  UI).
