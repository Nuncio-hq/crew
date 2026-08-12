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
reports Buzz version `0.5.3` and an updater must never downgrade it.

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

## D-018 — Scope managed agent execution by Project thread

- **Status:** Accepted
- **Date:** 2026-07-31

Each non-DM channel thread owns an independent ACP queue/session identity.
Top-level event IDs establish that identity; NIP-10 replies reuse the root ID.
The real NIP-29 channel remains separate for relay queries, reactions,
observer frames, membership cleanup, and thread-scoped typing indicators.

For an owner-authored Project task, Crew encodes the linked source workspace
as hidden composer metadata. Before a new ACP session, the harness validates
the source Git repository and creates one deterministic worktree and branch
from the immutable thread-root event ID. All agents handed work inside that
thread converge on the same path. Invalid metadata or worktree failure stops
the task instead of falling back to the source checkout.

A new multi-agent Project task notifies only the first explicitly ordered
agent. Later agents remain visible through non-notifying reference tags and
are woken by explicit mentions in subsequent thread replies. Ordinary chat,
single-agent prompts, DMs, and non-Project channels keep existing routing.

## D-019 — Hermes agents bind 1:1 to Hermes profiles; the profile owns the model

- **Status:** Accepted (Slice 1 of feature 0001)
- **Date:** 2026-08-05

Adopted from
[`features/0001-hermes-first-class-runtime.md`](features/0001-hermes-first-class-runtime.md)
(P-1 … P-7), backed by spikes 0009–0013:

1. Every Crew agent on the Hermes runtime binds 1:1 to a named Hermes
   profile. The profile owns model, provider, memory, skills, and
   credentials; Crew never stores a competing copy.
2. Crew renders model/provider for Hermes agents as read-only,
   profile-sourced information. The live ACP model switch remains a
   session-scoped escape hatch only.
3. For runtime=Hermes the spawn environment must not carry
   `BUZZ_ACP_MODEL` from any layer; enforcement is code once Slice 2
   lands (spike 0013 fixes the mechanism).
4. Tier-1 promotion of Hermes happens upstream in `block/buzz`; Crew
   carries at most temporary additive shims.
5. Hermes agents spawn with command basename `hermes` (or `hermes-acp`),
   never renamed wrappers, and select the profile with `-p <name>` args.
6. Crew may invoke `hermes profile create`/`delete -y` only as a direct,
   visible consequence of a manager action; deletion always passes `-y`
   and verifies by directory absence (spike 0011: bare `delete` on a
   non-TTY auto-cancels with exit 0).
7. The manager's default profile (`~/.hermes`) is never bound to a Crew
   agent without explicit confirmation. Public (`respond-to anyone`)
   agents additionally require a credential-isolation step: spike 0010
   showed fresh profiles read the manager's pooled credentials through a
   global-root fallback.

## D-020 — Hermes tier-1 promotion lands in Crew, not upstream

- **Status:** Accepted; supersedes D-019 item 4
- **Date:** 2026-08-05

The manager decided no pull request will be opened against `block/buzz`
for this feature (or, by default, for any Crew work). The Hermes tier-1
`KnownAcpRuntime` entry, the `default_agent_args` mapping, and the
preset removal are implemented directly in Crew on a branch cut from
Crew `main`, merged through the normal `NuncioCrew Gate`.

Consequences:

- Crew accepts a permanent fork delta in
  `desktop/src-tauri/src/managed_agents/discovery.rs` (and the sibling
  files the entry touches). Every upstream sync must re-verify this
  delta; conflicts there are expected and owned by Crew.
- The thin-fork budget in `UPSTREAM-SYNC.md` still applies to *how* the
  edit is made (smallest possible, no restyling, no drive-by changes),
  but "contribute upstream instead" is no longer the escape hatch.
- If upstream later ships its own Hermes tier-1 entry, the sync
  resolves in favor of upstream's shape and Crew's delta is retired.
- Feature 0001 documents (P-4, §7.2, Slice 3) are historical as written;
  this decision governs.

## D-021 — Keep Crew local workspace fields on upstream `Repository`

- **Status:** Accepted
- **Date:** 2026-08-05

Under NIP-MP, kind `30617` is a repository (`Repository`) and kind `30621` is
a project (`Project`). Crew's `buzz-location` tag and the derived
`localWorkspacePath` / `localWorkspaceStatus` fields live on `Repository` in
upstream's `projectModels.ts` — an intentional Crew edit of an upstream file,
recorded here rather than hidden behind a parallel type.

When a Project has several repositories, a Crew thread worktree binds to
`primaryRepositoryAddress`, falling back to `repositories[0]` for
`legacy: true` projects. Selection uses `selectProjectRepository()`.

## D-022 — Extract Crew deltas when sync trips the file-size ratchet

- **Status:** Accepted
- **Date:** 2026-08-05

When an upstream sync merge makes a shared file exceed the Desktop file-size
ratchet, extract the Crew-owned additions into new Crew-only files so the
shared file returns to at or below the upstream line count. Do not grant a
sync-only exception, do not raise `MAX_LINES`, and do not restructure
upstream's own code just to pass the guard.

This shrinks future conflict surface instead of freezing oversized shared
blobs. Record the extracted files in the sync PR body.

## D-023 — Crew-created Hermes profiles keep bundled skills

- **Status:** Accepted (Slice 4 / Phase 03 of feature 0001)
- **Date:** 2026-08-05

When Crew runs `hermes profile create <name> --no-alias` from the
create-in-place affordance, it does **not** pass `--no-skills`. Fresh
profiles receive Hermes' bundled skill set (~70 skills), matching the
CLI default observed in spike 0011.

Rationale: agents benefit from the standard skill set on day one; an
empty profile is a power-user CLI flow (`hermes profile create … --no-skills`)
rather than the Crew hiring path. Revisit only if managers ask for a
Crew toggle.

## D-024 — Keep profile-bound Hermes trusted, owner-only, and local

- **Status:** Accepted (Issue #104, Phase 01)
- **Date:** 2026-08-08

Profile-bound Hermes agents are trusted, owner-operated workers. Crew presents
their effective autonomy as **Full** and intentionally keeps the existing ACP
behavior: when Hermes sends `session/request_permission`, `buzz-acp` selects the
advertised `allow_once` option. Crew does not add a dangerous-command permission
inbox or a competing approval policy. Hermes clarification and elicitation
requests are product decisions, not permissions, and may use Crew's separate
**Needs You** flow.

Hermes' profile-owned `approvals.mode` remains authoritative. **Full** describes
Crew's ACP host behavior—it does not silently override a stricter Hermes profile
policy. An owner who wants profile-level prompt bypass changes that setting
through Hermes' canonical surface, not through Crew.

That autonomy has two mandatory backend boundaries:

1. A profile-bound Hermes agent is `owner-only`; `allowlist` and `anyone` are
   rejected even if a client bypasses the form.
2. A profile-bound Hermes agent runs on the local backend only. A local profile
   name is not a remote provisioning or secret-distribution contract.

Create/update commands and local start/provider deploy paths enforce both
boundaries. Existing invalid records remain loadable, stoppable, and deletable
so they can be repaired; the generic storage writer does not brick the registry.

Hermes profiles are local employees, not community-scoped copies. One local
managed-agent record owns runtime pairs for every configured community, so one
profile binding intentionally shares its memory, skills, and all other
profile-owned state across those communities. Crew makes that reach and the
shared-state consequence visible. A second local record cannot bind the same
profile: `ManagedAgent.relay_url` is a legacy pin ignored by effective relay
resolution and therefore cannot define occupancy. This is an
installation-local identity guard, not a network-wide profile lease.

This decision supersedes D-019 item 1 only where “1:1” could be read as global
uniqueness, and supersedes D-019 item 7's possible future public path for a
profile-bound agent. The profile still owns the model and credentials, and the
named-profile requirement remains unchanged.

## D-025 — Build on Buzz contracts; Hermes-first without parallel protocol

- **Status:** Accepted
- **Date:** 2026-08-10
- **Product doc:** [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md)

Crew keeps the Buzz backend (relay, Nostr identity, channels/threads, ACP
harness) and continues to fetch/sync upstream. Product work builds **on top**
of existing Buzz contracts.

Hermes is the **default optimized** agent runtime (profile-per-agent, memory
and skills on the profile). Other ACP engines remain welcome through the same
Buzz/ACP room contracts. Implementers:

1. Prefer existing kinds, mentions, sessions, and publish-back paths.
2. Extend wire contracts only when generic Buzz is proven insufficient;
   record the extension here.
3. Must not invent a Hermes-only parallel system for assignment or results.
4. Must not claim non-Hermes engines have Hermes profile memory.

Anti-lock-in (swap engines later) is enough; local-AI product investment is
out of scope unless a later decision supersedes this.

## D-026 — Mobile continues the same company; not a second product myth

- **Status:** Accepted
- **Date:** 2026-08-10
- **Product doc:** [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md)

Desktop is the main office. The mobile app continues the same workspace
(Need you, read threads, keep work moving). Do not split planning into
“Crew mobile product” vs “unrelated mobile app” as two identities: the first
mobile surface that matters is continuing this company on a phone.

Early mobile slices do not require desktop parity (Projects/PR/agent admin).
Do not rewrite Flutter → React Native solely for install/test ergonomics;
fix distribution or implement features on the existing client unless a later
decision supersedes this.

## D-027 — Plain-language agent collaboration with the founder

- **Status:** Accepted
- **Date:** 2026-08-10
- **Working agreement:** [`AGENT-WORKING-AGREEMENT.md`](AGENT-WORKING-AGREEMENT.md)

The founder is not a practiced company manager. Agents and implementers must
explain simply, label uncertainty about “real company” practice, refuse silent
mis-assignment when roles exist, and treat shared thread reports as the human
record. Spikes are not automatic law. Full text of MUST/MUST NOT lives in the
working agreement doc.

## D-028 — Roles are owner-assigned only ("email promote")

- **Status:** Accepted
- **Date:** 2026-08-10
- **Issue:** [Nuncio-hq/crew#116](https://github.com/Nuncio-hq/crew/issues/116)
- **Plan:** [`plans/20260810-agent-roles-routing-capability/plan.md`](../../plans/20260810-agent-roles-routing-capability/plan.md)

Only founder(owner)-signed role data has effect. Agents may propose a role
change in-thread; they never self-assign. Non-owner role events/tags are
ignored for prompt injection and authority checks. The original
managed-agent-record storage and `30179`/`10100` projection described below
were superseded by Slice 1R and are retained only as historical context:
roles now live as owner-signed `(agent, channel)` assignments in the channel's
Crew canvas block (D-043).

## D-029 — Capability is Crew-owned at spawn, keyed by role (not Hermes profile)

- **Status:** Accepted (policy; Slice 3 implements the floor)
- **Date:** 2026-08-10
- **Issue:** [Nuncio-hq/crew#116](https://github.com/Nuncio-hq/crew/issues/116)
- **Evidence:** spike 0017

Capability grant/deny is Crew-owned at the harness/session boundary and keyed
by the founder-authored channel assignment. Hermes profile ownership (D-024)
is unchanged and does **not** extend to being the capability boundary
(profile keeps memory/skills/credentials/model). The Slice 3 implementation
uses the per-session MCP list for `buzz-dev-mcp`; on engines with native
file/shell tools, channel denial remains a Crew rule rather than a hard wall.
Spike 0017's finding stands and is not softened: deny-dev-mcp alone is not a
filesystem floor for any engine with native file/shell tools. The earlier
proposal to pair the MCP grant with per-engine permission flags (Codex `-s`,
Claude permission mode) as a per-channel floor is superseded by Spike 0018,
which found those flags to be process-level; see D-044.

## D-030 — Routing presets reference roles, not hard-coded agent names

- **Status:** Accepted for future slice (Slice 2 implements)
- **Date:** 2026-08-10
- **Issue:** [Nuncio-hq/crew#116](https://github.com/Nuncio-hq/crew/issues/116)

Channel routing presets map work type → **required role**, never hard-coded
agent names (names allowed only as an explicit per-channel override). Resolution
role → current holder happens at read time. Non-founder preset edits are ignored
when Slice 2 lands.

## D-031 — Keep shipped state in sync with STATE.md

- **Status:** Accepted
- **Date:** 2026-08-10
- **Working agreement:** [`AGENT-WORKING-AGREEMENT.md`](AGENT-WORKING-AGREEMENT.md)

When a release is published, a slice merges, or the gate changes, update
`STATE.md` in the same PR. Repeated drift is costly because agents sequence
work from that file. Enforcement is review-visible prose, not a CI guard.

## D-036 — Evidence stays on existing message events as a tolerant tag

- **Status:** Accepted
- **Date:** 2026-08-10
- **Evidence:** [`spikes/0021-evidence-tag-roundtrip.md`](spikes/0021-evidence-tag-roundtrip.md)

Evidence uses `["crew-evidence", "<kind>"]` on existing message kinds. The
allowed values are `test-run`, `metrics`, `before-after-visual`, and `diff-stat`;
the first occurrence wins, and no new event kind is introduced.

This is a tag rather than a new kind because existing kinds can carry it while
other ACP engines and clients can safely ignore an unknown tag. The phase 01
spike recorded a real relay/storage/query/desktop round trip preserving the
tag and ordinary-client ignore safety.

The CLI appends the tag after the event is built and before signing, following
the precedent at `crates/buzz-cli/src/client.rs:590`. This keeps the additional
Crew delta out of both `buzz-sdk` and `buzz-core`.

Evidence is self-reported and can be fabricated, including numbers and test
excerpts. This raises the cost of lying and the odds of getting caught, but it
does not cryptographically verify work. Independent verification remains with
CI and PR review.

Only the CLI can emit the tag in this slice; the desktop composer and mobile
cannot. Only kind 9 renders an evidence card. Accept/Reject reuse existing
kind-7 reactions, using `✅` and `❌`; `KIND_AGENT_RECEIPT` ignores the tag and
keeps its receipt card.

The ≤30-line evidence bound is a prompt rule and a probe check, never a runtime
guard.

## D-032 — Keep Desktop Smoke E2E advisory until known failures close

- **Status:** Accepted
- **Date:** 2026-08-10
- **Verification:** [`verification/0007`](verification/0007-gate-e2e-shard-relationship.md)

The founder decided that Desktop Smoke E2E shards stay advisory and excluded
from `NuncioCrew Gate`. The trade-off is that `main` can merge with red E2E;
making a known-broken lane required would red-wall every desktop PR without
fixing a test. Over the last 10 `main` runs, shard 1 failed `8/8`, shard 4
cancelled at the 30-minute timeout `8/8`, shard 3 failed `2/8`, shard 2 passed
`8/8`, and the Gate was green `10/10`. Revisit making the shards required once
#109 and #110 are closed.

## D-033 — Record exact baselines for upstream-heavy files

- **Status:** Accepted
- **Date:** 2026-08-10
- **Issue:** #111

Upstream-heavy files are governed by an exact recorded baseline instead of the
hard 1000-line limit. `MAX_LINES` is not raised. Only upstream's own growth may
bump a recorded baseline; D-022 continues to govern Crew's own additions, so a
Crew-authored regression still requires a visible, reviewable manifest edit.
Recorded `lines` values use `wc -l` semantics, and the number to record is the
exact count printed by the guard.

The next upstream sync is expected to fail this guard once with a message
naming the new count. The sync PR remedies that failure with a one-line
baseline bump. This is deliberate and preferred over merge-parent
auto-detection machinery. Files already above `MAX_LINES` (for example
`desktop/src-tauri/src/managed_agents/discovery.rs` at 1494) retain the existing
implicit base-ref grandfathering and are not being migrated into the manifest
in this change; both mechanisms deliberately coexist.

## D-034 — Adapt upstream Project E2E specs to the outcome-first UI

- **Status:** Accepted
- **Date:** 2026-08-10

Crew adapts the upstream `project-*` Desktop Smoke E2E specs to the
post-#95 outcome-first Projects UI instead of skipping them and replacing them
with Crew-native tests under the #65 precedent. This is an accepted permanent
fork delta. Future upstream syncs must keep the adaptations, including the
Project Plumbing expansion helper and its call sites; resolve conflicts by
re-adding the helper calls when upstream refreshes those specs.

## D-035 — Offboarding archives a Hermes profile; deletion is archive-only

- **Status:** Accepted
- **Date:** 2026-08-10
- **Runbook:** [`HERMES.md`](HERMES.md)
- **Spike:** [`spikes/0015-profile-readiness-and-archive.md`](spikes/0015-profile-readiness-and-archive.md)

Offboarding a Hermes agent no longer runs `hermes profile delete -y`. The
destructive branch archives the profile to `<nest>/profile-archives` as a
`tar.gz` plus a sidecar manifest (profile, timestamp, bound agent name and
pubkey, optional reason, exclusions, sizes), excluding caches. Archiving is
copy → verify → remove: the live profile is removed only after the archive is
written and read back.

Permanent deletion exists only as an action on an archive and is gated in Rust
by an exact profile-name confirmation token, not by the dialog. Restore refuses
to overwrite a live profile of the same name. Every profile-destructive action
refuses while a runtime pair bound to that profile is alive — Crew does not
stop a working agent on the owner's behalf to complete a destructive action.

The reserved `default` profile and the `~/.hermes` root remain untouchable.

## D-037 — Channel-first stands; board deferred; work overview is future direction

- **Status:** Accepted
- **Date:** 2026-08-10
- **Product doc:** [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md)

1. **Channel-first stands.** Channels and threads are the main surface and
   where work happens. Board-as-home — columns as authoritative state, slot
   caps, and card-move-as-transition — is not current direction. This
   supersedes VISION.md § "Board as orchestrator" as a product commitment.
   [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md) remains the locked north star.
2. **Board schema stays deferred.** No board event kind or board tag schema is
   defined until a board-like surface is actually prioritized. This closes
   STATE.md's open decision "final board event kind and tag schema".
3. **The future direction is a work overview lens.** It is a read-only
   aggregation over signals that already exist as relay events: active turns,
   thread-workspace branch telemetry, Needs You, evidence and acceptance
   reactions from #121, and agent readiness from #119. It answers what each
   agent is doing, on which branch, what needs the founder, and what is done.
   It is a lens over existing events with no new authoritative state,
   consistent with D-003, D-010, and VISION.md's "board state = signed relay
   events". It is recorded as a future candidate track and is not in scope of
   this change.

## D-038 — Crew edits the bound Hermes profile write-through

- **Status:** Accepted
- **Date:** 2026-08-11
- **Issue:** #118

Crew is the editing surface for the bound Hermes profile's model and
`SOUL.md`; Hermes remains the single source of truth. Crew stores neither a
second model/persona copy nor a reset template. Reads come from the profile
files, and writes go through the Hermes CLI for model/provider values or the
profile file for `SOUL.md`. A successful write is read back before Crew
updates its display, and the model change applies on the next fresh ACP
session.

This supersedes the "no editable model control" presentation in rule 2 of
[`HERMES.md`](HERMES.md) and the corresponding D-019 note. The profile's
persona is Layer 1, `base_prompt.md` is the harness-owned Layer 2 office
rules, and the optional Crew agent description is Layer 3 job context,
appended only when non-empty.

Two sub-decisions are settled:

1. Crew does not offer "Reset to Hermes default": Hermes generates the
   default at profile creation, and Crew must not ship a copied default that
   can drift.
2. Provider and model changes use two separate `config set` invocations. If
   the second fails, the profile may be partially updated; Crew does not
   invent rollback without a verified Hermes unset operation.

## D-043 — Roles are channel-scoped owner-signed canvas assignments

- **Status:** Accepted (Slice 1R)
- **Date:** 2026-08-10
- **Issue:** [Nuncio-hq/crew#116](https://github.com/Nuncio-hq/crew/issues/116)

A role is a channel-scoped owner-signed assignment carried by the channel
canvas. This supersedes D-028's storage clause while retaining its authority
clause. Labels are free-form; the founder-authored definition travels with the
assignment and is the meaning used at read time. The former global managed-agent
role, 30179 extension, and 10100 projection are not enforcement sources.

## D-044 — Capability is founder-authored and channel-session scoped

- **Status:** Accepted (Slice 3)
- **Date:** 2026-08-10
- **Issue:** [Nuncio-hq/crew#116](https://github.com/Nuncio-hq/crew/issues/116)
- **Evidence:** spike 0018

The founder-authored `capabilities` map uses Crew-owned keys keyed by
free-form role label. Unknown keys and missing entries fail closed; the
day-one key is `buzz-dev-mcp`. Partial MCP tool allowlists and path
containment are out of scope.

ACP `mcpServers` is channel-session scoped, so engines whose only file/shell
path is dev-mcp receive a channel-session hard floor without respawning the
agent. Codex and Claude native file/shell tools remain process-level: their
containment is a process-level union and no per-channel native hard floor is
claimed. On those engines, denial is a Crew rule, not a wall.

**Amendment (2026-08-11) — the native-tool clause is narrower than written.**
Spike 0018's authenticated real-engine run showed Codex and Grok each enforce a
per-session native-tool floor on a single process: Codex honors
session-addressed `session/set_config_option` with `configId: mode` (a
`read-only` session was denied a file write while a sibling `agent` session on
the same PID wrote successfully), and Grok honors `session/set_mode`. The
process-level restriction is a property of Crew's spawn path (`agent_args` such
as Codex `-s`), not of those engines. "Denial is a Crew rule, not a wall"
therefore applies only to engines that expose no session-scoped permission
control, and must not be asserted for Codex or Grok. Claude is untested
(binary unavailable) and keeps the original wording until measured.

**Amendment 2 (2026-08-11) — implemented, so the enforcement claim is now
per-engine and observable.** Withholding `buzz-dev-mcp` for a channel clamps
that channel's ACP session to a read-only native floor as well, addressed by
session ID immediately after `session/new`
(`apply_native_tool_floor` in `crates/buzz-acp/src/pool.rs`): the engine's
advertised floor via `session/set_config_option` `configId: mode` when
`session/new` lists one (Codex `read-only`, Claude `plan`), otherwise an
attempted `session/set_mode` (Grok, which advertises no modes yet enforces
`plan`). Sibling channels on the same process are unaffected, so this is not a
process-level union.

Capability is still never inferred from the free-form role label, and the
floor is only clamped **down**: a channel with no Crew capability block keeps
the process-wide permission mode untouched.

An engine that refuses the request is not a failure — the session continues
with dev-mcp withheld and the harness emits a `session_capability_floor` frame
with `enforcement: "advisory"` (vs `"engine"` when the engine accepted). That
frame, not prose, is what any surface may claim. "Denial is a Crew rule, not a
wall" is therefore accurate exactly when `enforcement` is `advisory`.

## D-039 — Mission inbox snapshots memoize on inputs, not on the wall clock

- **Status:** Accepted
- **Date:** 2026-08-11
- **Issue:** [#135](https://github.com/Nuncio-hq/crew/issues/135)

`deriveMissionInboxSections` memoizes on a key built from its inputs. An
explicitly supplied `now` is part of that key; the implicit `Date.now()`
fallback is not. Identical inputs therefore always return the identical
snapshot object, which is what a `getSnapshot`-shaped selector must do.

The accepted consequence: without an explicit clock input, row `age` values
stay fixed until another input changes. The desktop caller passes no clock and
the home surface has no ticker, so ages already only refresh when a store
changes. A caller that wants clock-driven recomputation passes `now`.

## D-040 — Keep E2E mock fixtures production-authorizable

- **Status:** Accepted
- **Date:** 2026-08-11
- **Links:** [`userInputAttentionProjection.ts`](../../desktop/src/features/agents/userInputAttentionProjection.ts), [`userInput.ts`](../../desktop/src/features/channels/lib/userInput.ts), issues #110 and #130

E2E mock fixtures must emit only events that production could actually
produce. A fixture must not bypass authority rules—owner `p` tag, canonical
`e` causal parent and ancestry, 64-hex ids, or an owned-agent profile—to make
a red spec green. When production rejects a mock event, fix the fixture for
fidelity, reusing the shared `__BUZZ_E2E_EMIT_MOCK_USER_INPUT__` helper, rather
than weakening the production rule or the spec.

## D-041 — The E2E mock bridge mirrors relay event semantics, not per-surface shortcuts

- **Status:** Accepted
- **Date:** 2026-08-11

`desktop/src/testing/e2eBridge.ts` stands in for the relay in Desktop Smoke
E2E. When the desktop read model moved thread counts onto the relay's
kind:39005 recount push, the mock kept emitting that push only for huddle
ephemeral channels, so fifteen shard-4 specs failed on a missing
`message-thread-summary` for ordinary channels — a harness gap that read as a
product break. The mock also persisted kind:20002 typing indicators that the
relay discards as ephemeral (kinds 20000–29999), which inflated thread
descendant counts once the recount became visible everywhere.

The rule: mock relay behavior is derived from relay semantics for **all**
channels and event kinds, never narrowed to the surface that first needed it.
A mock that is selectively faithful produces failures that are indistinguishable
from product bugs, which is the expensive failure mode. When a spec fails on
missing mock-derived state, check the mock against the relay handler before
concluding the product is broken.

## D-045 — Unread accounting reads the reply graph, not the visible row list

## D-046 — Keep live-event filter relay-parity-safe

- **Status:** Accepted
- **Date:** 2026-08-11

Upstream's channel read-model overhaul made the timeline a projection of the
window store, and a non-broadcast thread reply is deliberately not a row in it:
replies live in the per-root `["thread-replies", channelId, rootId]` cache and
render inside the thread panel. Per-thread unread counts, however, were still
derived from the row list alone, so `(N new)` never appeared for a thread that
was not open — the replies the badge counts were, by design, not in the list it
counted (#152).

The rule: unread accounting consumes the reply **graph** — the timeline unioned
with the per-root reply caches, deduplicated by event id — while rendering keeps
consuming the row list. Restoring a count must never be done by widening the
visible timeline: that would undo the overhaul and put replies back in the
channel. Nor may a kind:39005 `descendant_count` stand in for it; the relay
recount is authoritative structural metadata and carries no per-reply identity,
so it cannot answer "which replies has this user not read".

A corollary on delivery: a live channel subscription bounded at `since = now`
drops any event whose `created_at` precedes it — a peer with a lagging clock, or
an event created between the window fetch and the subscription opening. The
client never learns such an event exists, so a reply can arrive for a root that
never renders. Channel live subscriptions therefore start a bounded grace window
before now (`CHANNEL_LIVE_BACKLOG_GRACE_SECONDS`), matching the existing huddle
TTS startup replay; the window store dedups by event id, so the overlap is free.

## D-042 — Relay-mode E2E mutations must reach the relay

- **Status:** Accepted
- **Date:** 2026-08-11
- **Evidence:** [`verification/0011-e2e-bridge-relay-mutation-audit.md`](verification/0011-e2e-bridge-relay-mutation-audit.md)

Mutating E2E-bridge commands that have a real Nostr event behind them must
publish through `submitSignedEvent` in relay mode, branching on
`getIdentity(config)`, and must skip mock-store bookkeeping. The relay event
must mirror the Rust command's kind, content, and tags exactly.

A mock-only mutation in relay mode is a coverage trap: the spec can still pass
while never reaching the relay. Any command that cannot yet be made
relay-aware must be listed in Verification 0011 with the reason; issue #144
tracks the remaining commands and the Rust-side confirmation of local-only
classifications.

This decision changes no reaction semantics, event kinds, or evidence tag
schema. D-036 remains authoritative for evidence tags and reaction behavior.

## D-047 — Keep Desktop E2E Integration advisory until Crew owns more of the lane

- **Status:** Accepted
- **Date:** 2026-08-12
- **Issue:** #147
- **Precedent:** D-032 (Desktop Smoke E2E advisory)

The founder decided that the relay-backed Desktop E2E Integration lane
(`playwright test --project=integration` in `nuncio-crew-ci.yml`) is **advisory**:
`continue-on-error: true`, excluded from `NuncioCrew Gate` `needs`, and absent
from `JOB_RELEVANCE` in `check-nuncio-crew-ci-results.mjs`. A green Gate is not
evidence that integration passed.

Rationale matches D-032's trade-off, applied to a heavier lane:

1. Only one configured integration spec is Crew-specific today
   (`evidence-reactions-relay.spec.ts` from #133 / PR #146); the rest are
   inherited Buzz coverage that Crew deliberately stopped requiring when the
   upstream `CI` workflow was disabled.
2. The lane needs Postgres, Redis, MinIO, a built `buzz-relay`, schema and
   community seed, `scripts/setup-desktop-test-data.sh`, Playwright, and
   `BUZZ_RECONCILE_CHANNELS=true`. Relay and service flakiness must not block
   every desktop merge.
3. Restoring the lane as advisory recovers continuous signal (including the
   D-042 headless click → real kind-7 proof) without red-walling merges.

Promotion path: making the lane required is an **explicit future founder
decision**, not an automatic follow-up when a single spec turns green. Revisit
once Crew owns a meaningful share of the integration suite and the lane's
failure rate on `main` is attributed and stable enough to gate on.
