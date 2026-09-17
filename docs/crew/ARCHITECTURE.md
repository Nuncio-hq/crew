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
ACP preparation and native worktree removal/pruning share a cross-process
metadata lease keyed by the canonical common Git directory. ACP waits at most
five seconds before a retryable preparation failure; native cleanup reports
contention before mutating Git state. Root/path authorization still applies,
and independent thread turns run concurrently after preparation.
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

## Local transport evidence boundary (#338)

The existing managed-agent process is the producer boundary for local transport
health. After native preflight, Desktop registers one exact generation ticket
(owner, runtime key, nonce, child PID, pre-spawn timestamp, wire version and
leave/rejoin epoch) and supplies the corresponding status path to ACP. A
generation that cannot establish the secure sidechannel keeps the legacy
health-only behavior; v1 records remain readable by a v2-capable native ticket.

ACP writes one bounded atomic latest record. The additive v2 shape carries the
process binding, a monotonic per-generation connection attempt, and only the
exact negative AUTH acknowledgement for that attempt. Desktop validates the
record against the ticket, lease freshness, attempt fence and fixed diagnostic
codes before projecting `transport`. The optional `transportAuthEvidence`
wrapper is derived from that accepted record and includes the native binding;
relay text and unrelated acknowledgements do not enter it. A failed generation
may receive a bounded final read in retired diagnostics, but the retired cache
cannot establish a live process or overwrite a newer generation.

The profile Activity row renders local transport health separately from process
lifecycle status.

The opt-in native evidence markers are a diagnostic sink only. Registration is
emitted before an AUTH marker, each marker is bounded and retried a fixed number
of times, and generation/attempt/AUTH identities deduplicate renewals and stale
replays. The producer-to-reader proof is an integration gate; its actual
built-ACP recipe is in [`TESTING.md`](TESTING.md#built-acp-producer-to-native-reader-proof).

## Observer completion scheduling (#352)

`ObserverPublishQueue::next_frame` retains queue-wide same-channel batching;
it does not publish one source event per tick. The Crew-owned
`observer_priority.rs` selects the oldest urgent lifecycle/control channel
within the prefix before the first null-channel barrier. A channel containing
both urgent events and ordinary output is urgent until those urgent entries
leave that prefix. Its causal predecessors still publish first. After two
urgent frame selections while eligible normal channels wait, the oldest
normal channel receives a frame. Classification is recomputed per slot, so
flush, eviction and completed gathers leave no stale channel metadata.

The publisher still emits at most one frame per one-second tick, under the
existing 120/min relay ceiling shared with chat. The plaintext frame cap,
4 MiB pending-byte budget and source-event drop accounting are unchanged.
Priority cannot move a terminal past its own over-frame backlog or any
null-channel barrier; fairness, relay quota and transport can also delay it.
The three-channel deterministic regression reaches completion in slot one
instead of slot three. This is queue/pacer evidence, not installed-runtime
latency: baseline/candidate timing and real-data screenshots await #348/#338.

Validated receipts already match the exact agent, triggering event, session
and turn when projecting a healthy thread's Ready to review badge. This is
distinct from observer-driven live control targets and founder acceptance.
Receipt-backed UI subscriptions observe the store's stable generation before
deriving aggregate review state. A receipt may arrive while its turn is still
active; an unchanged store must not produce a new external-store snapshot on
every read and cause a render loop in the channel's thread summary.
Error, disconnected and stalled attention retain their existing precedence.
During harness finalization, a task record can outlive its control receiver.
Cancel and model-switch report a sent signal only when the receiver accepts
it. A closed receiver preserves the existing fallbacks: cancel drains that
conversation's queued work or reports no active turn; model-switch reports
`turn_ending` while the task record remains. Receipts do not hide live controls
or change the model picker's live-switch/default-setting decision.

## CompanyOS prototype integration boundary

The #349 shell candidate mounts the existing Project sidebar projection and
channel browser beneath the workspace menu. Project expansion preserves its
repository coordinates; Wiki navigation carries the selected kind 30617
address, and channel entries use the existing channel IDs. Removing the old
channel groups from the visible sidebar does not delete their saved ordering
or membership. Project and Workflow preview definitions are retired per D-078.
The new Project Overview presentation is not yet mounted; its operation
controller and native recovery dispatcher remain #361 acceptance work.

The #361 relay candidate extends kind 9007 with the opt-in atomic creation and
exact-event recovery contract in D-080. Its durable state remains in Buzz's
channel, membership and event tables. All canonical discovery writers in this
binary share the fenced snapshot transaction. Deployment requires every writer
to be upgraded or quiesced before enabling `BUZZ_CREW_ATOMIC_CHANNEL_CREATE`;
leaving the flag off rejects signed opt-in requests without a legacy fallback.
The flag does not establish compatibility with older concurrent writers.

The shared [design reference](../../design/companyos/README.md) is simulated; its `index.html#feasibility` contains the current code/GitHub audit and proposed backlog consolidation. Existing production UI already mounts per-agent declared plans and a PTY-backed terminal; their new prototype tabs are presentation integration, not new engines. The audit also distinguishes current canvas assignment UI from bulk editing/contact routing and API-backed guided handover from the proposed runtime-backed user recap. Compact thread activity can reuse `conversationActivityHeadline` and the observer transcript projection: ACP session updates feed message/thought/tool items, coalescing chunks and updating tool rows by identity without another model request. This is an implementation path, not evidence that the new UI is connected.

Observer kind 24200 is ephemeral and owner-scoped, with NIP-44 encryption (`buzz-core/src/observer.rs`). A new card must preserve that access boundary, conversation/turn scoping and disconnected/missing-data states; channel membership alone does not grant access to raw telemetry. Runtime reasoning is displayed only when the adapter emits it. Complete history, multi-engine streaming and reconnect recovery require live verification.

Role labels resolve from owner-signed channel canvas assignments (`crew_role.rs`, D-043/D-044); thread owner and participants do not redefine them. The prototype's sample channel roster is not an authoritative registry or a capability source. Project backend model/recovery (demonstrated v0.9 UI accepted by D-078), queue/acceptance automation and external service integration are still proposals or unverified integration work.

The shared `buzz-core::crew_role` parser reads the first line-delimited Crew
fence; a malformed first fence cannot fall through to a later one. Definitions
retain display labels separately from their existing ASCII-normalized lookup
keys. Duplicate YAML keys (including unknown nested values), case-colliding
role/reference keys and invalid contact pubkeys fail closed. Legacy unquoted
numeric string keys are read directly from source, avoiding YAML numeric
conversion. `get_canvas` carries the event ID, unassigned definitions, optional
contact and explicit parse/author state to the desktop API. Foreign content is
visible for review; it does not gain execution authority.

`update_canvas_crew_config` is the pure bulk reconstruction seam;
`remove_canvas_crew_members` performs surgical cleanup using the same parser and
serializer. Both retain semantic unknown YAML,
tooling and exact outside-fence prose/later fences. Inside-fence formatting,
mapping order and comments may be normalized or lost by the YAML round-trip.
Renames rewrite exact assignment/routing/capability references; deleting a
defined role with unresolved references fails. Pre-existing dangling references
remain visible, and member cleanup preserves unrelated unresolved assignment
entries without choosing or dropping a holder. This helper performs no signing, publication, session reset or membership
change. Its caller supplies the event content bound; native
`events::build_set_canvas` independently enforces 64 KiB.

The #350 bulk editor uses `channel_crew_config` to prepare one owner-signed
kind 40100 canvas and one unmentioned kind 9 working-agreement notice. It records
both exact events in the shared owner-operation store before publication. The
captured owner/community generations, expected canvas head, bounded worker lease
and operation revision are rechecked at dispatch; retries read back exact IDs
before resending. Canvas acknowledgement with a pending notice remains a partial
commit. Applied status requires the canvas to remain the relay's current head;
a newer head is superseded, including after recovery. This is optimistic
readback protection, not distributed CAS: the relay can accept concurrent events.
Five failed attempts stop automatic recovery; manual retry retains the original
signed events. The native startup worker resumes due operations after restart.

`save_channel_crew_member_cleanup` shares this journal and publication path with
the deletion coordinator. The caller durably records its cleanup operation UUID
before calling and releases managed-store/process locks before awaiting it.
Retries first load that UUID and validate the original channel/member intent.
Only unchanged or applied completes the outer cleanup; other outcomes retain
recovery. This integration contract does not prove the full deletion workflow.

The #350 Manage roles dialog is reached through the existing channel Canvas
section. It loads current canvas and member data between native scope captures,
keeps role/contact edits in temporary form state, and fences queued results on
scope changes. Role candidates come from the relay-signed channel roster, so
stopped bot members do not depend on runtime directory hydration. Submit checks
the exact current channel and viewer membership again. Selected non-bot members
also require a verified managed identity, signed legacy agent record, or NIP-OA
profile; runtime and provider permissions are checked separately at execution.
It retains the draft on conflict or partial delivery, exposes
read-only status checks and explicit manual retry, and requires explicit draft
replacement after reviewing a newer canvas. Replacing a superseded draft first
removes that exact reconciled journal row through the owner-operation revision
fence; a failed removal keeps the recovery and draft available for retry.
Recovery lists put unresolved or partial operations ahead of reconciled
superseded history with a stable updated-time/ID tie-break. Raw unresolved
assignments remain visible and preserved until explicitly removed. Multi-holder
roles still use the existing one-role-per-agent assignment map; checking a
different role moves that agent, and removing an in-use role requires explicit
reference cleanup.
The existing raw `set_canvas` command remains a separate review/edit path without
an expected-head guard. Existing ACP sessions keep their cached canvas until
explicit restart; configuration saves do not hot-refresh those sessions. The
current candidate has technical native-UI and fresh/existing session-comparison
evidence for #350. Founder acceptance and release #357 remain separate gates.

The v0.6 prototype management flow reuses the canvas distinction between role definitions and holder assignments, including unassigned definitions. Its agent add/edit design maps to managed-agent create/update and Hermes profile discovery: Hermes binds a profile without editing its model. Optional initial channel joins are a separate lifecycle with acknowledgement/recovery, not an assumed atomic create. Git metadata maps to worktree registry/details and thread GitHub/forge projections; local changes against HEAD and PR changes against the base revision must remain distinct. These prototype flows are not connected to the commands yet.

The optional **Channel contact point** is `contact` metadata in the shared Crew
canvas parser, separate from assignments and capabilities. It is omitted when
unset and normalized to a canonical pubkey when present. Saving it does not
enable execution; #355 owns contact routing. Existing `crew_role.rs` routing presets map work types to role holders; they do not implement automatic wake-up. `buzz-acp/filter.rs` already supports channel/kind-scoped `SubscriptionRule.require_mention`, and `relay.rs` constructs `#h` / `#p` subscriptions. Implementing contact routing requires both subscription updates and inbound dispatch changes; turning off `require_mention` alone would also admit messages addressed to others and agent chatter. Preserve the separate `respond_to` author authorization gate.

The contact retention foundation stores tenant-scoped route evidence and quota
in `contact_routes` and `contact_quota`. Community deletion treats both as
ordinary scoped data: the catalog validates their write fences, and the purge
removes route evidence before quota rows before tombstoning the community.

Proposed behavior: human-authored channel messages and thread replies without explicit mention targets go to the selected contact; explicit mentions take priority and do not also wake the contact. Agent messages cannot trigger this fallback. The contact does not become the thread owner, gain a role, or gain tools. None leaves mention-only behavior. Missing, removed, or unavailable contacts require a visible unresolved state with no silent replacement. The real implementation must verify authors/structured mention targets, membership and canvas signer; retain kind/channel/access gates, deduplicate event delivery, fence stale subscriptions when contact changes, and handle acknowledgement/conflicts on save. Live validation must cover bot-loop prevention, mention priority, channel isolation, contact replacement/removal, reconnect and two-client edits. The prototype uses local atomic role/contact state and text matching for sample messages only.

Managed-agent deletion uses the existing `delete_managed_agent` command plus a
bounded owner-operation record. The coordinator snapshots the exact managed
record and channel membership, reserves one cleanup UUID per channel, then
stops and removes the local record under the managed-store/process locks. It
releases those locks before calling `save_channel_crew_member_cleanup`; the
outer record retains every unresolved channel step, key/tombstone step and
failure for startup recovery. Only unchanged or applied channel outcomes mark
the deletion complete; conflicts and exhausted retries remain reviewable.
The native journal validates this progression on every create and compare-and-
swap, while the renderer-facing generic create/update adapter rejects
managed-delete records entirely; only the native deletion coordinator can
advance them. A provider deployment checks the claim before invoking the
provider and again, under the transition/store fence, before persisting its
receipt, so a delete claimed during provider I/O cannot be overwritten by a
stale completion. Deletion also waits on the same per-agent provider lock
before capturing a remote record, so an in-flight deployment either finishes
before the deletion fence or is blocked by it. Spawn, restart, lazy reconcile
and launch restore all check the claim while holding the same transition/store
fence used to reserve deletion.
Workspace apply holds an outer transaction lock through launch restoration,
but releases the inner workspace scope lock before recovery captures its owner
scope. Queued workspace changes remain serialized while normal scoped reads
can complete. The one-shot restore latch clears only after successful,
non-shutdown restoration; failures retain it for a later apply retry. Directory,
status, and startup eligibility reads use persisted agent metadata without
hydrating every agent key. Restore and provider preflight hydrate selected
records and retain the existing missing-key refusal.
Canvas discovery pages signed kind 40100 history with the relay's composite
cursor and fails closed when the bounded scan cannot prove exhaustion. The
native recovery list exposes redacted unresolved summaries across communities;
status and retry require switching to the operation's captured community. The
deployed-remote guard and the existing Bestie assignment journal remain active.
The identity tombstone/archive is retained for historical display and active
pickers; it does not replace relay-authoritative channel cleanup.

Persona-card Delete reaches the existing `delete_persona` command. Its durable
parent captures the complete bounded target set before removing any instance;
each linked child uses the same instance deletion coordinator and cleanup
obligations. The persona definition is removed only after its children settle.
Failed stops, key cleanup, or tombstones retain parent/child progress for a
fresh process to retry. Restart recovery validates persisted process receipts
before stopping an untracked child; unsafe target receipts or a directory scan
exceeding 4,096 entries keep deletion pending without terminating a child.
Recovery initializes its captured retention directory before enqueuing
tombstones, including before normal retention hydration has run. Nested
coordinator futures are boxed to avoid exhausting a worker's
stack. Direct instance deletion retains its persona definition; persona-card
deletion explicitly removes that definition. Both retain message history,
runtime installations, worktrees, and role definitions. Hermes profile archival
remains a separate explicit choice in the existing confirmation.

The Agents directory reuses the existing persona and managed-instance queries. Each query failure exposes its own Retry action, retaining any cached cards while the failed query recovers. Instance Delete confirmation names the agent and is keyed by its public key: replacing the selected instance dismisses the confirmation, including same-name replacements. Renaming the same identity preserves its target. Cancel performs no removal. Relay-only rows use the existing policy-filtered relay query, exclude local instance keys and archived identities, and open the exact public-key profile without local management controls. Unknown local inventory blocks relay-only classification and offers Retry. Directory/profile Start and Restart, and profile Message, capture the community and signer for existing native scope assertions; component lifetime and target checks discard stale completions. Message pending state belongs to its captured scope, so changing scope permits a new operation and a retired completion cannot clear its pending state. These directory controls do not establish successful native process termination or canvas cleanup; those require separate runtime evidence.

Agent plans should reuse `declaredPlanSnapshot.ts` and `declaredPlanProjection.ts`, including ACP `sessionUpdate:plan`, structured todo fallbacks, complete snapshot replacement, explicit empty clears and retired-session filtering. Partition by agent pubkey and conversation; preserve owner-only observer access and last-known/disconnected/unknown states. Do not infer plan completion from process liveness or merge multiple agents' plans into a new authoritative task store. Runtime-matrix unit fixtures prove parser behavior, not live adapter emission. The new tab currently uses authored snapshots only.

User-facing thread recap is a separate proposed capability, default Off and generated on request. Reuse global settings/runtime discovery surfaces, but do not confuse `GlobalAgentConfig.handover_summarizer_model` (injected into guided handover as `BUZZ_ACP_HANDOVER_MODEL`) with a shipped multi-runtime recap service. The prototype Settings chooses an example installed runtime, Hermes profile or optional Codex/Claude model, without changing agent execution configuration. Production requires capability/auth validation, permitted thread-source collection, bounded generation with cancellation/error/retry, provenance and event watermark freshness, and acknowledgement-backed settings storage. Recap cannot redefine accepted scope or completion evidence. The prototype concatenates sample thread fields and does not call any model.

Stage 0 #344 source audit: existing Project.projectChannelId, relatedChannelIds
and buzz-related-channel already support home/related channel mapping; member
repository channels are existing data. No one-project-only invariant is approved.
The existing ACP `channel_membership` signal remains the sole membership source.
Its background subscription snapshot separates channel intent from readiness.
Readiness stays unknown until the startup command batch is applied, the
authenticated socket and membership watch are live, and channel/control replay
queues have recovered. A successful REQ write is local socket evidence, not a
relay acknowledgement, EOSE, or proof of work delivery.
Desktop projects it through the live observer ingress, retaining bounded rows per
normalized agent and harness generation. A row is readable only when the native
runtime status matches the agent and canonical community relay, carries the same
`startNonce`, is connected, and is not retired; missing, invalid, stale, or
disconnected input stays `unknown`. Community changes clear the projection, so
confirmed zero/nonzero counts stay distinct from startup or closed-watch
unknown without creating a parallel membership store. Retired-generation replay
fences are bounded and recyclable; the exact native nonce remains the authority
that makes a fresh generation readable after repeated restarts. #337's staging
recovery remains open. Receipt validation currently rejects mentionless direct
triggers (`receipt_parent_targets_agent`, ingest.rs); #355 must prove relay-authoritative
routing and durable execution/replay before its approved successor can ship.
Client-selected canvas/contact data alone cannot authorize a receipt.

## Project/Wiki v0.9 handoff boundaries (#344, #361–#367)

The accepted reference composes existing Project (30621), Repository (30617),
Wiki (30623 repository / 30023 company) and channel/thread models. #361 owns
exact selected-repository folder operations and legacy/zero/multiple mapping;
#349 owns scoped navigation. Prototype names and in-memory lists are fixtures.

#362 owns coherent publication/retention and shared G-DURABLE. D-079 accepts
immutable kind 30623 pages and manifests, a conditional TOC commit, and the
native `owner_operations` SQLite recovery journal. The #362 integration uses
`crew-wiki` to build and verify the complete signed graph, persists that graph
before transport, verifies immutable dependencies, and changes the TOC through
the relay's existing replacement transaction. Retries retain the signed event
IDs. Cadence changes reuse the verified manifest and pages. A lost TOC
acknowledgement remains unresolved until an exact live read establishes the
outcome; cancellation alone cannot prove that an attempted write did not land.
Cancel stops automatic sends. For an unresolved ambiguous attempt, explicit
Resume publication durably revokes cancellation before reusing the same graph
and CAS precondition. Reconcile remains read-only. A typed permanently retired
dependency requires Regenerate instead. When the relay proves that the exact
head, or the exact non-absent revision it required, was accepted and later
deleted, the attempt is settled terminally as superseded and its claim is
released in the same guarded write — no new state, action or network step. All
other absence remains unresolved; see D-079 for the full recovery contract.

The native `wiki_snapshot_read` command reads one exact repository coordinate
under a captured owner/community/generation token and rereads its replaceable
head after loading dependencies. The renderer shares one repository query and
read coordinator across Wiki consumers, with two concurrent native reads,
128 automatic repository reads per pass and a 64 MiB retained-graph budget.
Selection and explicit retry can prioritize repositories outside the automatic
set. An incomplete refresh can retain a previously verified graph only within
the same native scope, visibly marked stale; it cannot merge revisions.
Company kind 30023 knowledge remains an independent query and retains its
existing ACL. Push freshness uses the exact repository association and scoped
relay identity described in the D-079 clarification.

Writer/reader integration, restart recovery, and real cross-client acceptance
remain in progress. Signed relay events remain domain authority. Private Ask
history remains separate from shared publication recovery; this journal is not
a private history store or a channel-message outbox.

The accepted permanent-dependency recovery extension in D-079 adds a Wiki-only
atomic successor operation. Explicit Regenerate signs a new UUID-bound graph,
retains the unusable predecessor and transfers its local claim in one SQLite
transaction. Only direct predecessors of unresolved successors are pinned;
older ancestors follow normal retention. The exact relay refusal, fresh
dependency checks and locked relay CAS establish the outcome. A local check
before sending cannot revoke work already admitted by the relay. This requires
journal schema v4: the v1-to-v2 managed-agent claim migration, the v2-to-v3
Wiki successor migration, and the v3-to-v4 coordinator/child claim-index
rebuild are all atomic and idempotent. Older binaries fail closed, so recovery
preserves the v4 data and uses a verified compatible build rather than
restoring a stale v1 database. See [the recovery runbook](TESTING.md#wiki-journal-v4-recovery) and
[D-079](DECISIONS.md#d-079--owner-recovery-and-conditional-publication).

While the Wiki library is mounted, automatic refresh uses the same scoped
native generation/publication path as manual Update. A single effect-owned
timeout wakes the next eligible on-push debounce or daily/weekly deadline even
when query data stays unchanged. Scope, repository, job or input changes and
unmount cancel that timeout; unresolved durable claims still block new work.
One library-owned Buzz live subscription listens for kind 30618 from the known
repository owners or current relay self. Matching events only request a scoped
native snapshot reread; event payloads never become generation authority.
Subscription readiness and reconnect also reread, closing the initial history
gap. Bursts coalesce, and an event arriving during a read forces a trailing
read. Failures expose Retry live updates and pause cadence until recovery;
they do not start a refresh loop. Leaving the library retires the listener.

Initial Wiki generation reserves the existing Wiki publication claim before
starting a runtime. Its tagged `generation_version: 1` payload has no signed
pages and cannot enter the publication driver. A successful generation replaces
that payload with the verified signed graph using the same operation ID and
revision-checked update. Cancel settles the draft before signaling its temporary
runtime; a late completion cannot overwrite the canceled row. The native recovery
worker preserves a registered foreground generation and settles an interrupted
one with an explicit error once its process is gone. Restart never launches a new
runtime implicitly. This is a consumer payload extension within journal v3;
older consumers reject the draft rather than publishing it. Generation failure
or cancellation occurs before relay effects and releases the local claim so an
explicit Generate can capture fresh source.

#363 owns the installed-runtime generation seam and immutable Git/folder
snapshot handoff. The Wiki library and page header open a shared Generate/Update
Wiki dialog with the linked source, installed runtime, and Wiki-only profile/model
preference. Start saves that scoped preference before invoking native preparation;
a failed save keeps the dialog open and starts no runtime. Cancel or Escape before
Start dismisses the draft. Runtime labels load independently of opening the dialog,
and catalog refreshes preserve an edited draft. The picker reuses Buzz's complete
runtime catalog: Hermes uses its existing CLI availability; Codex requires an
installed underlying CLI independently of ACP adapter readiness; Claude remains
visible when its CLI is installed but is disabled for Wiki until compatibility is
verified. Library navigation resolves the
containing Project from the existing project query rather than using a repository
ID as a project route. Native publication resolves an owner/community/repository
scoped Wiki runtime preference, then starts a fresh bounded process through the
caller-agnostic `crew-wiki::Generator` seam. Hermes receives a copied named
profile, `HERMES_SAFE_MODE=1`, and the native `--safe-mode` flag; Codex receives
an optional model override, and a null model omits `--model` and
requests the isolated runtime default. For Hermes Agent v0.21.2 (source HEAD
`eec131b7163a8f287a9bddfc8ba11e6bd07ac49e`), the launcher loads profile dotenv,
external secret sources, and managed dotenv before its startup guard reapplies
safe mode; its `hermes_cli/oneshot.py` path still loads the selected profile
config directly to resolve provider/model. The adapter validates staged
`config.yaml` as missing/empty or a YAML mapping before launch, rejecting
malformed/non-mapping config with a fixed error. It also rejects profile
`.env`/`.op.env` routing assignments for `HERMES_HOME` or
`HERMES_MANAGED_DIR`, invalid or ambiguous dotenv bytes, and every enabled
external secret source. Accepted dotenv bytes remain unchanged, a missing
`.env` is created empty, and `HERMES_MANAGED_DIR` is bound to a fresh empty
directory under the disposable state root. Each installed Hermes page request
writes its native usage report inside that directory. Successful generation
requires a completed API call and valid effective provider/model identifiers;
values declared by the staged profile must match exactly, so a fallback cannot
be reported as the selected model. Missing or failed runtime execution is a
failed generation with no heuristic or HTTP fallback; the unsigned legacy
preview remains deterministic for compatibility. The adapter's state, prompt
input, stdout/stderr, telemetry, deadline, and cancellation are bounded, and
selection is independent from employee sessions and recap settings. The source
guard, config gate, profile-binding, and effective-runtime boundary are covered
by production seam tests in
`desktop/src-tauri/src/managed_agents/wiki_runtime_tests.rs`;
they also bind the installed command builder to the shared native containment
policy: macOS wraps the runtime in the fixed `sandbox-exec` process-fork denial,
Windows retains the bounded runner's Job Object, and unsupported Unix platforms
fail before disposable runtime setup. The fork-denial regression proves the
production command builder denies the fixture fork, but these tests do
not certify a native Hermes launch, provider/auth path, effective model, or full
installed tool isolation; Claude Wiki generation remains deferred until its
compatibility is verified. The #363 installed-runtime
acceptance must run in the #348 staging environment; #348 owns that environment
and #363 owns the runtime acceptance result. #364 owns scoped full-body
retrieval and exact-revision source reads. Desktop Wiki navigation persists the
selected page and bounded scroll position under the captured owner/community,
parent Project, repository coordinate, and door. A publication refresh may
replace signed page event IDs and V1 `p1-<hash>` address slugs. Navigation uses
the signed `wiki-slug` logical name (legacy pages fall back to their address slug)
to preserve selection across snapshots, while TOC links and source reads retain
the exact publication address and event ID. When the saved logical name is gone,
it selects a surviving page and
explains the fallback. Arrow-key navigation belongs to focused TOC page buttons
and moves focus with the selected page. Runtime dialogs, editable controls,
modified keys, and composition retain their own keyboard behavior. Source file controls are
fail-closed: they require a native grant in the current scope and an exact
recorded page reference, and never open current checkout bytes or arbitrary
line ranges when that evidence is unavailable. The source list and Markdown
citations both open a dismissible verified-source pane; the pane hides the
table of contents, returns focus to its activating control, and is scoped to
the exact owner, repository, path, and line range recorded on the page.
Native Update reads one verified prior publication and captures the new source
once. Installed runtime output is normalized before publication: a complete
Markdown-labelled outer fence is removed, while exact planned relative file
links become verified `buzz://file` links to that captured file's full range.
Markdown code spans and blocks remain literal, using the repository's Markdown
parser to distinguish examples from navigation. Unknown files and external navigation remain rejected; empty output and output
expanded beyond the runtime byte limit are not published. Terminal runtime errors
are shown as failed generation; cancellation and interrupted recovery retain the
canceled label and all terminal cases allow a fresh Generate. A no-op also requires
identical section membership and logical page order.
Detached Git snapshots match the absent branch tag emitted by the publisher;
an attached/detached transition still invalidates reuse. It reuses a page body only when source hashes and membership, page/section
metadata, language, source kind, branch, and the signed `wiki-steering-hash`
match. The runtime is created lazily for changed pages; an unchanged snapshot
makes no runtime calls and settles through the same durable generation claim.
New publications sign the complete steering-file digest (or `absent`) on the
TOC. Older heads without that digest regenerate once before reuse is possible.
Removed pages are omitted from the new manifest; the previous complete Wiki
remains readable until publication commits. Explicit recovery regeneration
builds fresh pages without reuse.

Native reads still require the selected root and live snapshot checks, and real
cross-client acceptance remains a separate gate. Repository announcements bind
their identity through signed kind 30617, author, and `d`; they need no self-`a`
tag. Wiki heads and manifests still require their exact repository `a` tag.
On macOS, an exact repository
read passes the retained root descriptor to the trusted `buzz-dev-mcp` /
`crew-wiki` multicall helper over stdin; the helper validates the directory,
changes directory by descriptor, and execs only the bounded read allowlist.
Missing helper support leaves folder reads available while Git revision reads
are unavailable. Linux retains its descriptor-bound `/dev/fd` path. #365
proves private existing-agent Ask without employee-session stealing or task
side effects; #366 consumes that proof and #367 owns explicit durable dispatch
and ACL-safe origin links. These are gates, not source-handoff implementations.

Coordinator-approved handbook compatibility (not a new founder claim): workspace-menu Company Wiki →
existing `goWiki()` / `/wiki` / `WikiLibraryScreen` Company Wiki card. Reuse
its kind 30023 content and ACL, with no new generator action, route migration
or company data removal. Verify the replacement in #349 before removing the sole old global
Wiki affordance; the reference library is sample UI only.

## Installed-runtime recap admission (#351)

Recap admission extends `KnownAcpRuntime` through `recap_contract`; it does not
create another runtime registry or borrow an employee's ACP session. Hermes
uses an explicit `recap_native_command` catalog value because ordinary ACP
discovery remains `hermes-acp` first. Other candidates use the existing
`underlying_cli` metadata. The native
CLI candidate and its model/profile selection contract are inventory, not proof
of one-shot support. `classify_recap` still returns only failure states;
discovery cannot create a positive capability. A typed bounded-probe
certification is the only input to the native producer, which persists a
redacted row in the existing scoped managed-agent retention DB (keyed by
runtime, executable fingerprint/version/platform, with a bounded Hermes
profile-tree digest when a profile is selected) and atomically projects the
strict runtime-ready grant. Production consumers require both the grant and
its matching retention row, so the registered command remains fail-closed
until an actual native probe and auth binding exist.
[#356](https://github.com/Nuncio-hq/crew/issues/356) remains dependent on a proven
runtime combination. Ordinary agent/ACP readiness is a separate contract.

The default-off source slice contains fixed Claude and Hermes candidate
argv/parsers, private disposable-state ownership, a native-only runtime-ready
grant loader/producer, and the existing bounded discovery process helper
extended with caller-owned stdin, cancellation and per-stream budgets. The
registered recap service binds those pieces at one production seam and keeps
its command unavailable without the grant and matching retention row.
Hermes binds the selected profile path, canonical directory device/inode
identity and bounded content digest. Plan construction copies the selected
profile into the disposable run; immediately before launch, the source and
destination are revalidated against the bound identity and digest. It disables
its configured MCP
path and rule injection, binds a one-shot prompt, and checks the written usage
model before accepting output. Empty toolsets, `--safe-mode`, or an ACP
read-only session do not certify tool isolation: the producer requires an
actual native observer rather than provider output claims; that observer must
bind the executed plan and report hostile-tool denial before effect plus an
unchanged controlled sentinel. No observer is wired yet, so the
certification entrypoint rejects the parsed envelope and cannot project a
positive grant. On macOS its plan is launched
through the fixed `sandbox-exec` process-fork denial policy; the ordinary Unix
process-group limitation and any escaped-descendant caveat still apply to the
bounded owner. Neither recipe has a positive staging grant in the current
inventory, so these adapter tests are not runtime acceptance.

The staging ownership loader reads only native `app_data_dir()` plus
`crew-staging-ownership-v1.json`. It verifies compiled demo identity, actual
process UID, canonical private owned roots and the excluded employee roots.
Manifest host/server strings are provenance, not launch authority. The five
roots must already exist; reading this receipt does not create them. A receipt
is tied to the root inode/device generations and revalidates before projecting
the recap state parent. A manifest claiming generation permission or containing
auth references is rejected. Ownership does not imply authentication readiness;
the producer still requires a native keyring binding, a completed bounded
probe, and the matching scoped retention row before it can project a grant.

Owner-local recap settings keep a safe Off value when their JSON is corrupt,
but the settings snapshot carries `settings_error: "invalid_settings"` so the
command caller can offer repair instead of treating the fallback as
authoritative.

The derived `agents` base must be canonical and owned before any run-directory
creation or recovery; an intermediate symlink is rejected, and active runs fence
base-generation replacements. Disposable generations live below
`agents/recap-runs/<uuid>`, use private files,
and persist a phase before a process may start. Prompt stdin is capped, opened
read-only and unlinked before launch. Cleanup refuses a pending-process phase;
startup recovery bounds its scan and preserves unknown, corrupted, replaced or
uncertain-process roots rather than guessing that a recorded PID is safe to kill.
A failed cleanup does not prevent attempts on other bounded entries, but its
first typed error still propagates. A `scan_limited` report means entries remain
unexamined; the 1,024-entry sweep is not proof of complete recovery. Repeated
unknown-entry flooding can delay reclamation, and pending-process roots require
separate verified ownership recovery. Non-Unix private-state ACLs are unproved
and rejected.

Thread source collection follows the bridge's chronological reply cursor with a
bounded forward scan (4,096 reply events plus a sentinel page). The source
builder then keeps the newest complete messages observed that fit the 256-event
and 128-KiB prompt limits. `source_overflow` is persisted in the recap manifest
when the scan ceiling is reached, and lookup reports such an artifact as stale,
so a bounded prefix is never mistaken for a proven newest/EOF snapshot;
auxiliary edits, deletions and reactions are excluded from this recap source.

### Bounded inventory limits (2026-09-10)

These observations describe the installed artifacts statically revalidated for
#351 at Crew HEAD `a179fc99e0558eda2b1ad35eab54b2d26c335336`, not permanent
limitations of the products. Earlier executed evidence is called out in the
rows; none is a successful recap generation. The #375 foundation is unchanged:
`classify_recap` returns failure states only; the native executor is present but
cannot admit a run without the separately bound runtime-ready grant.

| Candidate inspected | Static identity and evidence scope | Current blocker / execution status |
| --- | --- | --- |
| Claude Code image label 2.1.266, native macOS arm64 image, SHA-256 `553d1b9e9e7068b275c0a783c7e139ff6503096f286e674c8c919379fb0eca62` | Static hash matches the 2026-09-09 image; earlier isolated help/version and exact-image hook selection remain the executed evidence. | `unsupported_tool_isolation`: exact-image source shows managed hooks retained under safe mode/user hook-disable settings, with an in-process HTTP execution path; no native hook-denial test was run. |
| Codex CLI release-directory label 0.154.0, native macOS arm64 artifact, SHA-256 `4f85982624b3898c8991cb80c0981b2aa71070e3537046c9a95950318a95afcc` | Static hash only; no version/help ran on 2026-09-10. The historical 0.153.4 help/exec-help/schema observation is retired and not reused. | No certification for this artifact; one-shot/tool isolation remains unproved. |
| Goose, Mach-O `x86_64` artifact on the native macOS arm64 host, SHA-256 `8c38970cf68dd45df63f38d855ec00e215bc591a9aad547672c0b7a179c6f4c1` | Static hash only; version unprobed, with no native tool or descendant-containment proof. | No certification; native tool and containment behavior remain unproved. |
| Hermes installed mutable source declaring 0.21.1 in `pyproject.toml` | Read-only source inspection: `run_agent.py:95` calls `load_hermes_dotenv` with the installation `.env`; `env_loader.py:346-362` sanitizes/loads it even with isolated `HOME`. No Hermes process executed. | `unsupported_state_isolation`: the source declaration is not an executed version result; `hermes_cli/oneshot.py` imports the same AIAgent path. |

Mutable source, wrappers, executable upgrades, model, profile, platform or
enforcement changes invalidate any future positive proof.
Exact local path observations and one-run logs belong to #351/task evidence.

No candidate has an allocated recap profile, requested/effective model, or auth
grant; no strict descendant containment or recap generation was exercised. The
#348 ownership receipt remains inert and is not a runtime-ready grant. The
catalog still includes `buzz-agent`, but its `recap_contract()` exposes no
native one-shot command; no additional frontend/runtime list is inferred.

G-THREAD-1 v3 extends `toolPaneStore` with bounded scoped view selection, not a
second resource/session store. `ThreadFocusForgeSplit` and `ChannelToolPane`
consume exact channel/root-matched forge subjects; `FocusThreadDrawer` supplies
the outer Escape gate. `resetCommunityState` clears preferences; a small new
invalidation function binds confirmed mutation/query removal outcomes.
`useDeclaredPlansForThread` and existing observer/control generation projections
remain authoritative. Preserve `browserClose`/sim visibility cleanup; fence
`browserOpen` and `simEnsureDevice` mount activation. The existing channel-only
popout has independent window state and remains outside #354; governor resource
identity/leases remain shared. D-078 records the bounded approved decision.

The #354 control foundation extends the existing `cancel_turn` handler through
`crates/buzz-acp/src/crew_thread_cancel.rs`. An explicit `turnId` requires an
exact routing channel, conversation and current turn match; malformed or missing
matches cannot drain queued work or release an instrument lease. A closed signal
receiver is not a sent cancellation. Omitting `turnId` retains the existing
conversation cancel/queue-drain behavior. This does not stop the agent process.

`useChannelUserInput` retains the existing durable request validation and native
answer publisher. Its bounded ephemeral publication gate prevents same-view
duplicate answers and fences completion by relay, viewer, channel and ownership
generation. Publication failure keeps the request available with its error;
success waits for the existing durable resolution/claim semantics. Native
`QuestionRuntime` remains the authority for answer-versus-cancel claims. The
source-stage selected-run Stop and Steer controls are described below. Strict
Steer uses an additive `_session/steering` contract; it never falls back to
ordinary steering or starts a successor turn when the selected run is stale.

The #354 first slice implements the presentation in those seams: explicit Tools
opens Context, Activity reads the existing exact-conversation observer archive/live
merge, and Agent plans renders the unchanged D-056 projection in its own tab.
Recap remains visibly Off/unavailable. Historical Activity remains read-only;
the second slice adds explicitly selected current-run Stop and Steer plus the
scoped Need-you publication foundation. A retained plan without retained
observer events displays unavailable transcript history rather than fabricated
activity.

Selection is an in-memory LRU of at most 128 canonical relay/viewer/channel/root
keys. Navigation closes the presentation; explicit return restores only selection.
Community reset and confirmed channel/root removal invalidate it, while failed
channel queries preserve it. Narrow presentation uses the shared modal dialog and
the existing drawer Escape gate; the conversation DOM remains mounted. Browser
and existing simulator presentation require explicit activation per mounted thread
view, including after a scope round trip. Keyboard Browser/Sim shortcuts follow
the mounted thread's focus transition before opening its host, matching the Tools
button. The absent-simulator Create action reports pending/failure state and
activates the current thread scope after a successful boot. The mounted conversation
body owns one stable channel archive pager shared through the thread context with
Activity and Plans; the always-mounted activity peek reads the same observer store
without starting another pager. Once requested, hydration remains active for that
mounted thread across pane close and agent switches. Native cleanup remains
channel-owned.
Native presentation attempts fence late errors by generation; failed hide cleanup
stays visible in a channel-specific notice without disabling a newer activation.
The channel tool pane retains its mount/popout compatibility. Unit and mock-bridge
coverage establish view behavior only; installed staging evidence is still required
by #348/#357 before #354 is complete.


#354 selected-run Stop extends the existing encrypted observer control frame.
The source-stage `send_scoped_observer_control` command captures the native
owner scope and requires the caller's exact token, including identity/workspace
generations. Observer control kind 24200 uses Buzz's existing WebSocket path,
not the HTTP event-ingest bridge. Its publisher retains the captured WebSocket
URL and signing keys, uses shared rate-limit admission and NIP-42 authentication,
and rechecks `assert_current` after connection setup immediately before sending.
The complete attempt has a ten-second budget. No journal is added. Failure
before the control event is sent is `not_attempted`; an exact positive relay acknowledgment
is `accepted`; all uncertain sent outcomes remain `unknown`. Relay acceptance is
not a harness Stop acknowledgment. The existing owner-authorized harness handler
still verifies the exact channel/conversation/turn target. A scope change after
send cannot retroactively turn that attempt into `not_attempted`.

Clean `ControlSignal::Cancel` termination still emits `turn_error` so active-turn
projections close, but carries `outcome: "cancelled"` with generic `error: "Run stopped"`;
the desktop renders a stopped status. Missing or malformed terminal outcome data
continues to render as a genuine error.


The source-stage Activity run picker uses existing live observer session/turn
identities and captures the native owner token before enabling selection.
A turn may start before its runtime session exists. A later `session_resolved`
frame fills an empty session identity only for the same active agent, channel,
conversation and turn; it never replaces an already bound session. The live-run
projection is invalidated when that identity becomes available.
No run is selected implicitly. Ending or replacing a selected run retains its
identity as unavailable rather than targeting its successor. Stop requires the
same current channel/conversation/session/turn and target ownership at click;
ordinary ownership refetches or unrelated membership changes preserve pending
claims. Revocation retires them. Results correlate agent, turn and request ID,
and stale view completion cannot settle a newer request. Steer carries the same
captured scope together with the exact session, turn, request UUID and text
prompt. The native adapter routes it only to the matching task and advertises
the strict capability before the client selects that transport. `buzz-agent`
admits at most eight queued requests, caps text at 16 KiB, deduplicates 128
request IDs, and appends only at its round boundary. A shared terminal claim
arbitrates bounded expiry against that append, so an expired request cannot be
appended later. The receiver remains attached across an optional plan
continuation and is cleared when the task returns to the pool. Strict outcomes
are `appended`, `stale_target`, `rejected`, `busy`, and `expired`; an uncertain
publication remains unconfirmed and is never replayed automatically. Every control
frame is answered: a malformed frame is rejected with a bounded reason rather
than dropped, and a queued request the adapter never received settles as
`stale_target` (replay-safe) instead of `unconfirmed`. The boundary is the write
to the adapter, not the shape of the answer: a prompt that ends while a written
steer is still pending settles as `unconfirmed`, because the adapter may already
have applied it. Publication
feedback unlocks retry only for confirmed `not_attempted`; a correlated adapter
terminal outcome also releases the claim. A correlated rejection carries the
adapter's own reason — control-character scrubbed and bounded to 240 bytes —
into the control result and the Steer feedback line, with generic feedback when
the adapter supplies none. The client wait is deliberately longer than the
adapter's own request deadline, and the correlation outlives that wait, so a
late terminal outcome still downgrades an unconfirmed request to a retryable
one. Unconfirmed delivery keeps its claim and never enables a blind retry;
Steer and Stop latch independently, so an unconfirmed Steer never removes Stop.

The live job desk under the thread head is the one thread-level control strip.
Bound to the same owner-scoped publication seam as the Activity tab, it names
the working agent and offers Steer and Stop for an exact run when the thread has
exactly one live run this viewer owns; it points at Activity when several are
live, because choosing a target for the operator would be guessing. When no run
identity is known yet — or the live run is not this viewer's to control — the
desk keeps its agent-scoped controls, whose labels name the agent they act on:
Steer focuses the composer, Stop stops that agent. The Activity tab keeps the
full run picker, and the thread-wide control is named "Stop all runs" so the
three scopes cannot be confused. The transcript remains mounted alongside these controls;
installed workflow acceptance remains tracked by #354/#357.

## Installed Wiki runtime authentication (#363)

The installed Wiki adapter uses disposable runtime state and hands off only
the existing subscription access credential. Hermes authentication remains
owned by its selected staged profile. Claude reads the current user's native
Keychain entry (`Claude Code-credentials`) through the stable `/usr/bin/security`
helper, with a five-second deadline, cancellation, and bounded captured streams.
Captured credential output is never logged. Only the unexpired access token is
passed in the child environment; the refresh token is never copied. Codex disables
shell execution/snapshots, agent delegation, image tools and web search in the
disposable invocation. Installed CLI request capture verifies the advertised
tool catalog separately from generation or native acceptance. Codex reads
the host `CODEX_HOME/auth.json` and writes a private child file containing the
access, identity, account, and exact `last_refresh` fields plus the parser
required empty refresh field; API keys and refresh tokens are omitted. Missing,
malformed, oversized, or expired credentials fail before launch. The child
configs use fresh state and no user settings. Claude uses restricted mode,
strict MCP config, and an empty tool list; Codex retains read-only sandboxing
and explicit shell feature disables. These flags are launch controls, not proof
of Codex snapshot-only or complete tool isolation; installed generation and
credential renewal remain acceptance requirements.

## Private Wiki Ask lineage (#365)

A private Ask is the third member of the desktop's native one-shot family, after
the #351 installed-runtime recap and the #363 installed Wiki runtime. It shares
their shape — discover an installed runtime, certify it against a retained
probe, then run it once in a disposable root under a Seatbelt policy — and adds
the fences a viewer's private question needs.

`desktop/src-tauri/src/managed_agents/private_ask.rs` owns the adapter; its
submodules split the responsibilities:

| Module | Responsibility |
|--------|----------------|
| `launch.rs` | The fixed native plan: argv, isolated environment, run root, policy wrapper |
| `prompt.rs` | Persona as delimited authority, question and grounding as data, config fingerprint |
| `containment.rs` | The Seatbelt policy text for one run root and one runtime directory |
| `capability.rs` | The only production producer of a positive capability, from a retained probe |
| `selection.rs` | The resolver's deciding half: one native observation of an agent becomes a selection, or a typed refusal |
| `selection_native.rs` | The resolver's gathering half: the agent record, the live harness generation, one scoped snapshot read, the source grant |
| `binding.rs` | The sequencing from a resolved selection to an answer: retained-or-fresh probe, projection, admission, attempt |
| `citations.rs` | The answer-citation fence: every cited path must be one the verified grounding accounts for |
| `history.rs` | The bounded owner-local record of what was asked and what came back |
| `cancel_registry.rs` | Which attempts are in flight, and the flag `private_ask_cancel` raises on one |
| `session_evidence.rs` | Observation of a running employee session's ACP session-ledger directory; the only way to obtain isolation evidence |
| `profile.rs`, `recovery.rs`, `validation.rs` | Hermes profile staging, crash recovery, input fences |

The lineage difference that matters is the transport: a recap and a Wiki
generation publish their result, and a private Ask publishes nothing. There is
no event kind, no command that writes to the relay, and no store that mirrors
the question or the answer outward. The question, the answer and the history stay
on the viewer's machine. The history is durable there and nowhere else: a
bounded newest-first window capped by count and by age, in an owned 0o700
directory, written 0o600 through a temporary file and a rename — the owned-run
retention shape. Refusals are kept beside answers, and only citation paths and
line ranges are stored, never the source text.

That absence is enforced rather than documented. Every relay-bound egress
boundary in the desktop calls the key-backup guard in `egress_guard.rs` — the
`EVENTS_INVENTORY` scan fails the build if a new one does not — so the guard is
a complete census of relay traffic from the desktop's own identity. The private
Ask proof brackets a whole attempt through the production launch path and
requires that census to move by zero.

Certification is separate from discovery. `PrivateAskCapability` carries one
`ProofStatus` per property — authentication, tool isolation, read bound, egress
bound, process containment, side-effect freedom, independent invocation — and
`admit_private_ask` refuses on the first that is not `Verified`, naming it. A
discovered runtime is always unverified.

`private_ask/probe_run.rs` is the sole producer. It launches a probe program
Crew ships under byte-identical policy text to a production answer, with the
attempt proxy serving, and measures every effect from the desktop's own side
rather than accepting the child's report of itself. `private_ask/probe_receipt.rs`
retains that trace in an owned, uid-validated directory bound to this install's
ownership digest, so one probe serves later Asks until it expires; loading a
receipt yields a probe, never a capability, and every admission fence still
applies. `independent_invocation` is deliberately never restored from a receipt.

`private_ask/selection.rs` is what turns an observation into a selection. A
private Ask names an agent, a repository and a question; everything admission
checks — the persona and model from the agent's own effective configuration, the
executable identity, the ACL projection, the harness generation and the verified
snapshot — is produced natively in `private_ask/selection_native.rs`, which is
also the only part that needs an `AppHandle`. Its one piece of relay traffic is a
**read**: the same scoped kind-30623 query the Wiki pane makes, so the snapshot
is verified here rather than carried by a renderer. Nothing is published.

Session isolation is observed from the harness's own ACP session-ledger
directory for the exact (relay, agent) pair, together with the PID of the child
this process owns a handle to. At least one ledger entry is required; a missing,
empty, oversized or unreadable directory refuses before any child starts. The
desktop does not depend on `buzz-acp`, so that directory's location is a mirrored
derivation pinned by a test. An agent with no live harness generation therefore
cannot be asked at all — there is nothing to be shown to have been left alone.

That observation brackets the **answering run**, not the capability probe: the
ledger digest, entry count and owning PID are read immediately before and after,
and an answer produced beside a session any of them moved under is refused
rather than returned. It is deliberately not part of the probe receipt, because
it is a fact about one run beside one session rather than a property of the
machine — which is exactly what lets a fresh receipt restore the containment
dimensions and lets a second Ask on the same selection answer without spawning a
probe child.

`private_ask/binding.rs` is what turns a resolved selection into an answer, and
`private_ask/citations.rs` is what bounds that answer: the runtime prints its
citations in a fixed Markdown footnote form the prompt states, and an answer
citing a path the verified grounding cannot account for is refused outright.
Previously the response simply echoed the request's grounding, which said
nothing about the answer.

The resolver in front of the binding is `private_ask/selection_native.rs`, so
the developer surface reaches a real selection or a typed refusal. See D-083 for
what each producer observes and for the named limits.
