# Crew State

## Issue #187 — workspace binding per thread

Composer selector on git Project channels: New worktree (default, pickable
base) / Main checkout (`ws=main`) / Existing branch (`ws=branch:`). Absent
marker params keep today's isolated worktree. Path-keyed exclusive turn
leases serialize shared checkouts with a named Busy refusal. Dirty main
checkout is allowed with a prompt notice. Canonical checkout is never a #174
GC candidate; shared idle is `max(lastUsedAt)`. Non-git folders are refused
at add-Project (spike 0024); Cowork is #188. See D-053.
Last updated: 2026-08-13

## Issue #189 — upstream sync Buzz Desktop 0.5.10 → 0.5.11

Pinned to `desktop-v0.5.11` / `248b9d1b7666aacbcb1485b76e81de30a271ba0e`.
Idle sleep is one composed seam (D-052): upstream eligibility/race reaper
feeds Crew Ready → Draining → Listening with resume-first `session/load`.
Also adopted: standard ACP usage, channel description context, observer
envelope batching, foreground-ready scheduler (+ #164/#174 hooks), profile
component split (Hermes re-homed), timeline #5662 + #167, community deletion
engine/migrations. See sync PR for the overlap verdict table.
Last updated: 2026-08-13

## Issue #175 — evidence–CI cross-check badge

Evidence cards compare machine-readable `test-run` / `diff-stat` claim lines
(`Tests: <N> passed, <M> failed`, `Diff: +<A>/−<D> across <F> files`) against
the thread PR's existing GitHub status (checks + additions/deletions/
changedFiles). Badge states: Matches CI / Diverges (shows both values) /
CI running / Not comparable. Metrics and before-after-visual are permanently
Not comparable. Accept/Reject unchanged — badge is never a gate. Amends D-036.
Last updated: 2026-08-12

## Issue #174 — worktree storage reclaim (completes #59 P3)

Settings → Storage aggregates managed worktrees with cache/checkout split, PR
state, dual idle clocks (observed vs wall), and refusal-aware rows. Idle
candidacy uses an app-scoped alive-interval ledger + pure `observed_idle`
(default 48 observed hours) or merged PR (registry state, never git ancestry).
Suggest-and-confirm bulk Lean/Hibernate runs over existing #72 commands; no
background auto-GC. See D-051.
Last updated: 2026-08-12

## Issue #173 — session compaction awareness + guided handover

Honest per-engine `CompactionSignal` adapters update ledger `compaction_count`
(Known only; Unknown/Unavailable never show a number) plus a turn-count safety
net (default 100). Crossing threshold 3 (configurable 1–10) projects a benign
**thread banner** with owner-triggered guided handover: per-app summarizer
model, `["crew-handover", model]` note card, ledger `OwnerReset`. Summarizer
failure degrades to informed blind reset. See D-050 and spike 0023.
Last updated: 2026-08-12

## Issue #180 — session ledger compaction / rotation awareness

Hermes `sessionProvenance` and Codex compacted markers update ledger
`rotation_count` + optional `lineage` tips. Wake re-validates after
`session/load`: if the ledger lags the engine or the lineage tip mismatches,
resume is refused and the #169 fail-closed rebuild + delta path runs. Owner
aging UI / guided handover is #173 / D-050. See D-049.
Last updated: 2026-08-12

## Issue #169 — idle engine spin-down + resume-first wake

Local managed agents spin down their engine/MCP pool after
`BUZZ_ACP_POOL_IDLE_TIMEOUT` (default 30m; desktop local pairs pass it) while
the harness stays Listening with presence/subscriptions/buffering. A durable
session ledger declares session ids at `session/new` birth; wake resumes via
`session/load` when the engine advertises `loadSession` and validation passes,
otherwise rebuilds fail-closed. Agent cards show **Sleeping · wakes on
mention**; Mission Inbox excludes Sleeping; wake feedback uses the typing
indicator seam. See D-048 and spike 0022. Compaction-awareness: #180 / D-049.
Last updated: 2026-08-12

## Issue #116 Slice 1R — channel-scoped roles

Roles are founder-authorized `(agent, channel)` assignments stored in the
channel's signed `KIND_CANVAS` event inside a fenced `crew` YAML block.
Assignments carry a free-form label and founder-authored definition text.
The harness resolves them when creating a fresh channel session and ignores
non-owner or malformed canvas blocks. Channels without an assignment retain
the existing prompt behavior.
Last updated: 2026-08-12

## Founder product direction (docs)

Locked narrative for agents (not a shipped feature checklist):

- [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md) — company-on-machine, Hermes-first
  on Buzz contracts, mobile continuity, in/out scope
- [`AGENT-WORKING-AGREEMENT.md`](AGENT-WORKING-AGREEMENT.md) — plain language,
  honesty, no assumed manager experience
- Decisions **D-025**, **D-026**, **D-027**, **D-046** in [`DECISIONS.md`](DECISIONS.md)
  (upstream **D-024** remains Hermes trusted/owner-only/local)

Implementation slices below remain the code truth for what is built today.

## Repository

- GitHub: `https://github.com/Nuncio-hq/crew`
- Fork parent: `https://github.com/block/buzz`
- Default branch: `main`
- Baseline upstream commit: `248b9d1b7666aacbcb1485b76e81de30a271ba0e`
  (`desktop-v0.5.11`; see [`upstream-buzz.json`](upstream-buzz.json))
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
  the normal reply composer;
- test-run / diff-stat cards show a live evidence↔CI cross-check badge against
  the thread PR (Matches / Diverges / CI running / Not comparable); metrics and
  before-after-visual stay Not comparable (#175).
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
- relay-backed reaction, deletion, and canvas publication in the desktop E2E
  bridge, with a relay-mode evidence-card Accept/Reject spec;
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
- Settings displays the pinned Buzz version `v0.5.11 · Local`; the
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
  `0.5.11` / `desktop-v0.5.11` at
  `248b9d1b7666aacbcb1485b76e81de30a271ba0e`.
- The protected Environment, reviewer, nine encrypted release secrets, updater
  public variable, and Nuncio updater keypair are configured.
- Signed dry run `30537460233` and publish run `30538712572` passed.
- The public DMG is signed, notarized, stapled, ARM64-only, and launch-tested
  from the mounted image. Real-profile relay and Project acceptance remains a
  manager test after manual installation.

## CI lane

- Required merge signal: `NuncioCrew Gate`.
- Upstream-heavy file-size exceptions use exact per-project recorded baselines;
  `MAX_LINES` remains 1000, and upstream syncs must review any one-line
  baseline bump while D-022 governs Crew-authored growth.
- Automatic checks: desktop fast gate, unsigned macOS ARM64 package, and a
  path-filtered real-relay Project contract.
- Advisory (non-blocking) on desktop path changes: Desktop Smoke E2E (D-032)
  and Desktop E2E Integration — real relay + Postgres/Redis/MinIO,
  `playwright --project=integration`, two shards (D-047 / #147). A green Gate
  is not evidence either advisory lane passed. Integration expected-failure
  inventory (accepted Buzz drift + Crew-owned `evidence-reactions-relay`
  strict-mode fix) lives in [`CI.md`](CI.md) (#171).
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
- Verification 0011 records the closed headless click-to-real-relay reaction
  path (#133), the #144 remaining-mutation pass (user-input answer, persona
  publish, identity archive, managed-agent message), and the #172 workflow
  relay branches (30620 / 46020 / 5). Local-archive commands stay confirmed
  local-only.
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
- A bare `?thread=<id>` channel deep link opens the thread panel on that head
  even when the head is not already in the loaded timeline: the panel is held
  open while the channel route resolves that exact head, and still closes once
  resolution settles without it (deleted or bogus ids) or when an unrelated
  open head goes missing.
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

- Shard-4 Desktop Smoke E2E is revived: the upstream Project specs were
  adapted to the post-#95 outcome-first Projects contract.
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
- The desktop E2E mock relay now matches the real relay where user-input
  authority depends on it: emitted kind `46040` requests are durable, their
  canonical causal root is retained, the owner-declared relay agent is in the
  agent registry, and generated event ids are 64-hex. `channels.spec.ts ›
  channel question card accepts an answer` passes again (issue #110); the two
  remaining `channels.spec.ts` failures (sticky date divider, channel agent
  activity indicators) are unrelated and predate this change on `main`.
- `channel-agent-presence.spec.ts` now passes its needs-you smoke-shard
  scenario in 12/12 clean runs; the fix was an E2E fixture-fidelity gap, not
  a product break (issue #130).
- `scroll-history.spec.ts` *fast middle-page scroll settles with continuous
  mounted coverage* now passes; the failure was a harness setup bug, not a
  virtualizer/page-fetch product break. The test live-emitted backdated
  messages to force a prepend, but `mergeLiveChannelWindowEvent` correctly
  drops events below the open oldest boundary (they wait for ordinary relay
  paging). Setup now seeds older mock history and wheels up for a real
  older-page prepend before the middle-scroll coverage asserts (issue #155).
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
- **Implemented (#139):** Exact local snapshots refresh on app focus with a
  debounced point-in-time re-read via
  `useLocalWorkspaceSnapshotFocusRefresh` (trailing 500ms debounce + 5s min
  interval; invalidates active `local-repo-snapshot` queries only). The D-015
  exact reader path is unchanged. File-watcher liveness remains a future
  upgrade tied to the work overview lens in D-037(3).
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
- Whether symlink-selected workspaces should remain unsupported or get a
  separately reviewed canonical-path flow.
- When to publish or link a real Project on the manager relay for the final
  native exact-reader smoke.

## Hermes runtime track (feature 0001)

- Profile readiness now has a Crew-owned Hermes evaluator and named status
  projection: binary-missing, missing, broken-config, and neutral
  auth-unknown. Healthy profiles remain outside the generic setup
  requirement pipeline until Hermes provides a truthful headless auth probe.
- Hermes profiles now have backend archive, restore, estimate, listing, and
  confirmation-gated permanent-delete contracts. Archive is copy-verify-remove,
  excludes recorded transient cache directories, and refuses while a bound
  runtime pair is alive.
- Hermes profiles now have backend archive, restore, estimate, listing, and
  confirmation-gated permanent-delete contracts. Archive is copy-verify-remove,
  excludes recorded transient cache directories, and refuses while a bound
  runtime pair is alive.

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
- Slice 2 profile binding/readiness and Slice 4 lifecycle are shipped across
  #104 and #134. Issue #118 adds the profile model/provider write-through
  editor, exact-byte `SOUL.md` editor, skippable persona-at-birth step, and
  optional Layer-3 instructions. Hermes remains authoritative: L1
  `SOUL.md` is the Hermes-owned profile persona, L2 `base_prompt.md` is
  harness-owned office rules, and L3 is optional per-agent Crew job context
  appended only when non-empty.
- Remaining: a truthful Hermes headless auth probe, live session model
  discovery, and the upstream tier-1 sync work described in the feature plan.
- Issue #119 profile lifecycle hardening now includes named Hermes readiness
  projection, generic preflight/nudge routing, Crew-owned archive/restore and
  permanent-delete backend contracts, readiness indicators, archive
  offboarding, and an Agents-page archive browser. Archive semantics follow
  D-035 and spike 0015; real Hermes authentication remains unverified here
  because no Hermes binary is installed.

## Agent roles track (issue #116)

- Plan:
  [`plans/20260810-agent-roles-routing-capability/plan.md`](../../plans/20260810-agent-roles-routing-capability/plan.md)
- Slice 0 spikes **0015–0017 PASS** (records under `docs/crew/spikes/`).
- **Slice 1R (channel-scoped roles) — implemented on branch**
  `feat/issue-116-agent-roles`: owner-signed `(agent, channel)` assignments
  in the channel canvas `crew` block; free-form labels and carried definitions;
  fresh-session harness injection; no global role projection or taxonomy.
  Slice 2 routing presets are implemented in the canonical Rust parser and
  injected into fresh channel-session context with exact work-type matching,
  resolved holders, and explicit founder escalation for unheld roles.
  Slice 3 capability is implemented with founder-authored role capabilities,
  channel-session dev-mcp selection, and per-session native-tool floors where the
  engine advertises session-scoped control (Codex, Grok, Hermes tested; Claude not yet).
- Next: orchestrator review → PR; no partial MCP allowlists or path containment.

## Advisory smoke baseline cleanup (2026-08-12)

Consistent smoke fails on main fixed without weakening assertions:

- **channel-activity-popover** — unread live sub now uses
  `CHANNEL_LIVE_BACKLOG_GRACE_SECONDS` (same as timeline live) so lagged self
  roots populate `authoredRootIds`.
- **channels sticky day divider** — harness seeds two recent local days into
  the mock store before open (2023 live emits were outside grace / never
  rendered).
- **channels agent activity empty** — `getAgentObserverSnapshot` forces
  `connecting` only when restored events lack live contact; empty+open is idle.
- **evidence-reactions Reject** — selector scoped to `message-timeline` card
  (thread-head dual render is intentional after Reject opens composer).

## Advisory integration drift inventory (2026-08-12)

Issue #171 — lane stays advisory (D-047). Expected failures are listed in
[`CI.md`](CI.md) so red means a new problem outside that set:

- **Fixed:** `evidence-reactions-relay` Reject strict-mode — same
  `message-timeline` card scope as smoke PR #170 (relay variant was out of
  #170 scope).
- **Accepted upstream drift** (resolve via
  [`UPSTREAM-SYNC.md`](UPSTREAM-SYNC.md), no sprawl issues): consistent
  `agents.spec.ts` catalog/create/overflow cases, `profile.spec.ts`
  runtime-tab + Inbox badge cases, and `integration.spec.ts` live mention
  home-feed refetch cases. Confirmed on runs 31567147317 / 31573328800 /
  PR #176.
