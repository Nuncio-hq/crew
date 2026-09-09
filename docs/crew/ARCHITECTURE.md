# Crew Architecture

## Relationship to Buzz

Crew keeps Buzz's relay, Nostr identity model, desktop shell, channels, and ACP
harness. Crew adds a manager-facing orchestration layer.

Product intent and unimplemented CompanyOS work are in
[`PRODUCT.md`](PRODUCT.md) (D-076). This diagram names integration boundaries,
not a claim that the complete department/queue/delivery loop is shipped.

```text
Manager
  |
  v
Crew office surfaces: channels/threads, live-job desk, Tool Pane,
repository/workspace context, Hermes hire (React/TypeScript on the Buzz shell)
  |
  v
Buzz relay (signed events; shared state)
  |
  +--> channels and card conversations
  +--> project, worktree, receipt and evidence events
  +--> agent mentions
  |
  v
buzz-acp -> provider ACP adapter -> coding agent
                                  |
                                  v
                         local filesystem tools
```

The relay is the shared coordination log. It is not the source-code store.

### Company deployment

The founder chose the self-hosted dev-server over Tailscale
(`ws://100.86.143.13:3000`) as the relay for this company's deployment and
acceptance work. This choice does not restrict Crew's multi-community support:
users can configure other relays, including hosted communities.

The deployment choice does not establish current relay health, the installed
build, or each client's effective target. Verify those on the endpoint used
for an acceptance run; repository support for an event kind does not prove
that an installed relay accepts it. Historical transport observations and the
remaining reconnect/status work are tracked in
[#338](https://github.com/Nuncio-hq/crew/issues/338); isolated staging is
tracked in [#348](https://github.com/Nuncio-hq/crew/issues/348).

## Three data planes

| Data                                 | Authority         | May leave the machine through |
| ------------------------------------ | ----------------- | ----------------------------- |
| Messages, receipts, evidence, plans, roles | Local Buzz relay  | Relay WebSocket               |
| Working directory and source code    | Local filesystem  | Nothing by default            |
| Images, video, and large artifacts   | Local media store | Uploaded media URL            |

Do not put source trees into relay events or media uploads. Do not make React
state authoritative for relay-backed state (D-003 / D-010).

## Project identity and location

Buzz already represents a repository with a NIP-34 announcement, kind `30617`.
Its identity is:

```text
(announcement author pubkey, d-tag identifier)
```

Location metadata answers where the repository can be accessed. Existing
examples include clone URLs. Crew adds a local workspace location to the same
announcement; it must not create a second identity or replace the `d` tag.

The first local-only slice uses this approved extension:

```text
["buzz-location", "local", "<raw absolute path>"]
```

It has these invariants:

- existing NIP-34 clients continue to see a valid repository;
- `(pubkey, d)` remains the Project identity;
- `clone` remains Git transport metadata and is never replaced by the path;
- unsupported location types and durable unknown tags survive relink;
- there is exactly one active `local` location after an explicit link;
- malformed or duplicate local records fail closed;
- the manager sees the relay destination and plaintext warning before publish;
- cold reload reads the path from the relay, not React or a separate database.

After Project load, the desktop checks the selected location through Buzz's
existing read-only Git snapshot command. A missing or unusable workspace is
reported as unavailable without changing Project identity or relay metadata.

## Project and channel

Upstream already binds a Project announcement to a Buzz channel with the
`buzz-channel` tag. Crew treats that channel as the Project's coordination
context instead of inventing another Project-room identity.

A Project may therefore supply:

- stable NIP-34 identity;
- one or more locations;
- one Buzz channel;
- card conversations and assignments related to that Project.

## Folder-first Project creation

NuncioCrew keeps creation relay-native while making the manager interaction
folder-first:

```text
Projects + / Repository
  -> native directory picker
  -> path, relay, and Project-name review
  -> exact (owner, d) duplicate query
  -> create or reuse canonical Project channel
  -> sign and publish kind 30617
  -> fetch exact signed event id
  -> validate owner, d, channel, and local path
  -> insert the confirmed Project into the query cache
```

Cancel returns before channel creation or relay publication. A failed
publication keeps an in-memory retry token scoped to the full `(owner, d)`
identity. If acknowledgement succeeded but read-back timed out, retry accepts
the exact matching relay event instead of misclassifying it as a duplicate.

The read model retains the validated local path and canonical channel. A
local-only Project has no synthesized clone URL, so Projects overview and
terminal actions cannot silently clone it. The separate Local workspace strip
is removed; workspace association is part of Add Project.

If the app exits after channel creation but before successful Project
publication, the in-memory retry token is lost and the channel may be orphaned.
Durable orphan reconciliation is outside this slice.

## Exact local Git reader

For a linked workspace, Crew reuses the existing Buzz native command without a
Rust change:

```text
buzz-location/local absolute path
  -> TypeScript dirname + basename
  -> existing get_project_local_repo_snapshot
  -> native canonical containment and .git checks
  -> require returned path == selected path
  -> render read-only repository snapshot
```

The exact-path comparison is an isolation boundary. If resolution returns any
other folder, Crew rejects the result. Project overview also stops after an
unavailable linked-path read, so it cannot silently load a same-named
configured checkout or remote repository.

The snapshot supplies files, README, commits, contributors, and language data.
Linked Projects do not expose clone, fetch, pull, push, Terminal, or commit
diff because those existing paths still resolve through clone metadata or the
configured Buzz repositories directory. Enabling them requires a separate
exact-path mutation spike.

The existing native containment rule rejects a selected path whose final
component is a symlink escaping the supplied parent. Crew reports that case as
`Local unavailable`; it does not canonicalize around the guard.

## Current ACP workspace boundary

The early absolute-path-only phase is no longer the complete contract.
[`pool.rs`](../../crates/buzz-acp/src/pool.rs) resolves and leases eligible
Project-thread workspaces through `resolve_and_bind_channel_workspace`,
then supplies the bound path to session creation/loading. Ordinary non-Project
turns retain the harness cwd. Existing cached sessions and resume have their
own binding checks; a new path is not an instruction to mutate an active session.

Workspace preparation failures and missing/busy workspaces have explicit
outcomes. The implementation lives in `thread_workspace/`, `pool.rs`, and
`crates/buzz-worktree`; inspect those seams before altering isolation or cleanup.
Provider-specific behavior requires current verification, not inference from
the original feasibility experiments.

## Coordination authority and lifecycle

Crew state must be reconstructible from relay events after restart or on a
second client. React may cache a projection but cannot own state.

The board-as-orchestrator, column schema, and fixed `Working <= 3` product
rule were superseded by D-037. They are not requirements for CompanyOS.
D-076 prioritizes a verified coding delivery loop; it does not choose a new
queue schema or remove runtime resource limits.

Model coordination on existing threads, named mentions, roles, receipts,
and session/workspace signals first. Crew owns company coordination;
Hermes owns employee execution, profile memory, skills, and tools. Shared
company state must not become a second authoritative runtime-local store.

## Session lifecycle

The lifecycle design separates durable work from runtime resources:

- create when work begins;
- persist enough identity to resume;
- stop provider processes when no turn is active;
- restore context when a later mention or manager response arrives;
- never require a process to remain blocked while waiting for a person.

Task history must remain understandable even if a provider session cannot be
resumed or a checkout has been removed. Acceptance, thread archival, process
shutdown, and worktree deletion are distinct operations. The CompanyOS
retention and automatic cleanup policy remains open in `PRODUCT.md`; this
section does not authorize deleting unfinished work.

## CompanyOS prototype integration boundary

The shared [design reference](../../design/companyos/README.md) is simulated; its `index.html#feasibility` contains the current code/GitHub audit and proposed backlog consolidation. Existing production UI already mounts per-agent declared plans and a PTY-backed terminal; their new prototype tabs are presentation integration, not new engines. The audit also distinguishes current canvas assignment UI from bulk editing/contact routing and API-backed guided handover from the proposed runtime-backed user recap. Compact thread activity can reuse `conversationActivityHeadline` and the observer transcript projection: ACP session updates feed message/thought/tool items, coalescing chunks and updating tool rows by identity without another model request. This is an implementation path, not evidence that the new UI is connected.

Observer kind 24200 is ephemeral and owner-scoped, with NIP-44 encryption (`buzz-core/src/observer.rs`). A new card must preserve that access boundary, conversation/turn scoping and disconnected/missing-data states; channel membership alone does not grant access to raw telemetry. Runtime reasoning is displayed only when the adapter emits it. Complete history, multi-engine streaming and reconnect recovery require live verification.

Role labels resolve from owner-signed channel canvas assignments (`crew_role.rs`, D-043/D-044); thread owner and participants do not redefine them. The prototype's sample channel roster is not an authoritative registry or a capability source. Project backend model/recovery (demonstrated v0.9 UI accepted by D-078), queue/acceptance automation and external service integration are still proposals or unverified integration work.

The channel role editor prototype uses a local snapshot only. Its production write seam is `get_canvas` / `set_canvas` (kind 40100); implementation must preserve unrelated canvas content, routing and capabilities and resolve edit conflicts. The present command signature accepts channel/content without an expected revision, so concurrent-edit safety is not established by reusing the command alone. Show success only after relay acknowledgement; retain the draft on failure. This UI does not add channel members or grant tools.

The v0.6 prototype management flow reuses the canvas distinction between role definitions and holder assignments, including unassigned definitions. Its agent add/edit design maps to managed-agent create/update and Hermes profile discovery: Hermes binds a profile without editing its model. Optional initial channel joins are a separate lifecycle with acknowledgement/recovery, not an assumed atomic create. Git metadata maps to worktree registry/details and thread GitHub/forge projections; local changes against HEAD and PR changes against the base revision must remain distinct. These prototype flows are not connected to the commands yet.

The v0.7 prototype adds an optional **Channel contact point**, proposed and not implemented in the backend. Store one member pubkey with the owner-signed channel canvas snapshot, alongside roles while preserving unrelated data. This extends the existing canvas schema rather than creating another registry. Existing `crew_role.rs` routing presets map work types to role holders; they do not implement automatic wake-up. `buzz-acp/filter.rs` already supports channel/kind-scoped `SubscriptionRule.require_mention`, and `relay.rs` constructs `#h` / `#p` subscriptions. Implementing contact routing requires both subscription updates and inbound dispatch changes; turning off `require_mention` alone would also admit messages addressed to others and agent chatter. Preserve the separate `respond_to` author authorization gate.

Proposed behavior: human-authored channel messages and thread replies without explicit mention targets go to the selected contact; explicit mentions take priority and do not also wake the contact. Agent messages cannot trigger this fallback. The contact does not become the thread owner, gain a role, or gain tools. None leaves mention-only behavior. Missing, removed, or unavailable contacts require a visible unresolved state with no silent replacement. The real implementation must verify authors/structured mention targets, membership and canvas signer; retain kind/channel/access gates, deduplicate event delivery, fence stale subscriptions when contact changes, and handle acknowledgement/conflicts on save. Live validation must cover bot-loop prevention, mention priority, channel isolation, contact replacement/removal, reconnect and two-client edits. The prototype uses local atomic role/contact state and text matching for sample messages only.

The v0.8 prototype adds agent deletion, recap settings and an Agent plans tab. Deletion maps to the existing `delete_managed_agent` lifecycle in `commands/agents.rs`: stop process, recover/clear assignments, remove the record/key and enqueue identity tombstone/archive. Preserve the deployed-remote guard and the existing higher-level deletion orchestration. The prototype retains an identity tombstone for historical display and projects active pickers/roles/contact from it; this local projection does not establish successful relay cleanup. The proposed canvas contact field must join a durable cleanup/retry flow before shipping.

Agent plans should reuse `declaredPlanSnapshot.ts` and `declaredPlanProjection.ts`, including ACP `sessionUpdate:plan`, structured todo fallbacks, complete snapshot replacement, explicit empty clears and retired-session filtering. Partition by agent pubkey and conversation; preserve owner-only observer access and last-known/disconnected/unknown states. Do not infer plan completion from process liveness or merge multiple agents' plans into a new authoritative task store. Runtime-matrix unit fixtures prove parser behavior, not live adapter emission. The new tab currently uses authored snapshots only.

User-facing thread recap is a separate proposed capability, default Off and generated on request. Reuse global settings/runtime discovery surfaces, but do not confuse `GlobalAgentConfig.handover_summarizer_model` (injected into guided handover as `BUZZ_ACP_HANDOVER_MODEL`) with a shipped multi-runtime recap service. The prototype Settings chooses an example installed runtime, Hermes profile or optional Codex/Claude model, without changing agent execution configuration. Production requires capability/auth validation, permitted thread-source collection, bounded generation with cancellation/error/retry, provenance and event watermark freshness, and acknowledgement-backed settings storage. Recap cannot redefine accepted scope or completion evidence. The prototype concatenates sample thread fields and does not call any model.

Stage 0 #344 source audit: existing Project.projectChannelId, relatedChannelIds
and buzz-related-channel already support home/related channel mapping; member
repository channels are existing data. No one-project-only invariant is approved.
Existing membership signals/badges need readiness repair (#337), not a parallel
membership store. Receipt validation currently rejects mentionless direct triggers
(`receipt_parent_targets_agent`, ingest.rs); #355 must prove relay-authoritative
routing and durable execution/replay before its approved successor can ship.
Client-selected canvas/contact data alone cannot authorize a receipt.

## Project/Wiki v0.9 handoff boundaries (#344, #361–#367)

The accepted reference composes existing Project (30621), Repository (30617),
Wiki (30623 repository / 30023 company) and channel/thread models. #361 owns
exact selected-repository folder operations and legacy/zero/multiple mapping;
#349 owns scoped navigation. Prototype names and in-memory lists are fixtures.

#362 owns coherent publication/retention and shared G-DURABLE. Publishing TOC
last alone cannot preserve old replaced pages. There is no established generic
Project/Wiki journal, private Ask history store or message outbox in the audited
desktop; the concrete bounded owner-local seam must be approved, with private
history separated from signed-event recovery. No new authoritative domain store
is implied by this handoff.

#363 certifies installed-runtime generation and immutable Git/folder snapshots;
#364 owns scoped full-body retrieval and exact-revision source reads. Current
local file reads use worktree bytes and cannot prove historical source. #365
proves private existing-agent Ask without employee-session stealing or task
side effects; #366 consumes that proof and #367 owns explicit durable dispatch
and ACL-safe origin links. These are gates, not source-handoff implementations.

Coordinator-approved handbook compatibility (not a new founder claim): workspace-menu Company Wiki →
existing `goWiki()` / `/wiki` / `WikiLibraryScreen` Company Wiki card. Reuse
its kind 30023 content and ACL, with no new generator action, route migration
or company data removal. Verify the replacement in #349 before removing the sole old global
Wiki affordance; the reference library is sample UI only.
