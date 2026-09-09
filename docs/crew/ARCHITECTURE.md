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

UI integration remains tracked by #350 and is not included in the native
checkpoint. The dialog must retain its draft on conflict or partial delivery,
expose status and manual retry, and require explicit replacement after reviewing
a newer canvas. Raw unresolved assignments must remain visible and preserved
until explicitly removed.
The existing raw `set_canvas` command remains a separate review/edit path without
an expected-head guard. Existing ACP sessions keep their cached canvas until
explicit restart; configuration saves do not hot-refresh those sessions. Full
#350 relay and fresh/existing session acceptance remains outstanding.

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

The v0.8 prototype adds agent deletion, recap settings and an Agent plans tab. Deletion maps to the existing `delete_managed_agent` lifecycle in `commands/agents.rs`: stop the process under the managed-store/process locks, recover/clear Bestie assignments through its existing journal, remove the record/key and enqueue identity tombstone/archive. This journal does not yet perform relay canvas role/contact cleanup. Preserve the deployed-remote guard and the existing higher-level deletion orchestration. The prototype retains an identity tombstone for historical display and projects active pickers/roles/contact from it; this local projection does not establish successful relay cleanup. Canvas role/contact removal must join a durable cleanup/retry flow before agent deletion can claim that cleanup.

The Agents directory reuses the existing persona and managed-instance queries. Each query failure exposes its own Retry action, retaining any cached cards while the failed query recovers. Instance Delete confirmation names the agent and is keyed by its public key: replacing the selected instance dismisses the confirmation, including same-name replacements. Renaming the same identity preserves its target. Cancel performs no removal. Relay-only rows use the existing policy-filtered relay query, exclude local instance keys and archived identities, and open the exact public-key profile without local management controls. Unknown local inventory blocks relay-only classification and offers Retry. Directory/profile Start and Restart, and profile Message, capture the community and signer for existing native scope assertions; component lifetime and target checks discard stale completions. Message pending state belongs to its captured scope, so changing scope permits a new operation and a retired completion cannot clear its pending state. These directory controls do not establish successful native process termination or canvas cleanup; those require separate runtime evidence.

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

## Installed-runtime recap admission (#351)

Recap admission extends `KnownAcpRuntime` through `recap_contract`; it does not
create another runtime registry or borrow an employee's ACP session. Hermes
uses an explicit `recap_native_command` catalog value because ordinary ACP
discovery remains `hermes-acp` first. Other candidates use the existing
`underlying_cli` metadata. The native
CLI candidate and its model/profile selection contract are inventory, not proof
of one-shot support. `classify_recap` currently returns only failure states;
there is no positive capability cache, generation command, or runnable recap UI.
[#356](https://github.com/Nuncio-hq/crew/issues/356) remains dependent on a proven
runtime combination. Ordinary agent/ACP readiness is a separate contract.

The default-off source slice contains a fixed Claude candidate argv/parser,
private disposable-state ownership, and the existing bounded discovery process
helper extended with caller-owned stdin, cancellation and per-stream budgets.
The Claude recipe remains unapproved for generation. Its native `--tools ''`
flag is not sufficient to establish that all hooks are disabled. A Unix process
group bounds ordinary descendants but does not contain a `setsid` escape;
reader shutdown is not evidence that an escaped process exited. No production
OS-sandbox recipe is enabled.

The staging ownership loader reads only native `app_data_dir()` plus
`crew-staging-ownership-v1.json`. It verifies compiled demo identity, actual
process UID, canonical private owned roots and the excluded employee roots.
Manifest host/server strings are provenance, not launch authority. The five
roots must already exist; reading this receipt does not create them. A receipt
is tied to the root inode/device generations and revalidates before projecting
the recap state parent. A manifest claiming generation permission or containing
auth references is rejected. Ownership does not imply authentication readiness;
a future, separately reviewed runtime-ready grant is still required.

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

### Bounded inventory limits (2026-09-09)

These observations describe the installed artifacts examined for #351, not
permanent limitations of the products. None is a successful recap generation.

| Candidate inspected | Evidence scope | Current blocker |
| --- | --- | --- |
| Claude Code 2.1.266, native macOS arm64 image, SHA-256 `553d1b9e9e7068b275c0a783c7e139ff6503096f286e674c8c919379fb0eca62` | Isolated help/version and exact-image hook selection inspection | `unsupported_tool_isolation`: managed hooks survive safe mode/user hook-disable settings, including in-process HTTP hooks. |
| Codex CLI 0.153.4, native macOS arm64 image, SHA-256 `b973d440acac501fd2594a43e7ca9ce41e0a65b9dfb28d0d7a7837c99e1261e3` | Isolated help/version/exec help and locally generated app-server JSON schema | `unsupported_tool_isolation`: exhaustive native tool denial was not proved by the available controls/schema. |
| Hermes installed source declaring 0.21.1 in `pyproject.toml` | Read-only source inventory; no Hermes process executed | `unsupported_state_isolation`: CLI import enters installation repair; both CLI and `run_agent.AIAgent` import paths load the installation `.env` independently of disposable HOME. `hermes_cli/oneshot.py` imports the same AIAgent. |

The installed Hermes source location was the user's `.hermes/hermes-agent`
checkout; its declared package version is not a binary fingerprint or an
executed version result. Mutable source, wrappers, executable upgrades, model,
profile, platform or enforcement changes invalidate any future positive proof.
Exact local path observations and one-run logs belong to #351/task evidence.

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
source-stage selected-run Stop UI is described below. Strict Steer remains
unimplemented: the existing ordinary-message `_session/steering` transport can
start a new turn and cannot promise rejection of a stale selected run.

The #354 first slice implements the presentation in those seams: explicit Tools
opens Context, Activity reads the existing exact-conversation observer archive/live
merge, and Agent plans renders the unchanged D-056 projection in its own tab.
Recap remains visibly Off/unavailable. Historical Activity remains read-only;
the second slice adds explicitly selected current-run Stop and the scoped
Need-you publication foundation. Strict Steer and additional workspace
instruments remain unavailable. A retained plan without retained observer events displays unavailable
transcript history rather than fabricated activity.

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
generations. It uses the shared `OwnerOperationTransport` with captured keys and
origin, and passes a lazy `assert_current` check that runs after admission.
It neither adds a journal nor duplicates the transport. Native preparation or
admission rejection is `not_attempted`; an exact positive relay acknowledgment
is `accepted`; all uncertain sent outcomes remain `unknown`. Relay acceptance is
not a harness Stop acknowledgment. The existing owner-authorized harness handler
still verifies the exact channel/conversation/turn target. A scope change after
send cannot retroactively turn that attempt into `not_attempted`.


The source-stage Activity run picker uses existing live observer session/turn
identities and captures the native owner token before enabling selection.
No run is selected implicitly. Ending or replacing a selected run retains its
identity as unavailable rather than targeting its successor. Stop requires the
same current channel/conversation/session/turn and target ownership at click;
ordinary ownership refetches or unrelated membership changes preserve pending
claims. Revocation retires them. Results correlate agent, turn and request ID,
and stale view completion cannot settle a newer request. Only a confirmed
`not_attempted` publication unlocks retry; an unknown send remains unconfirmed.
The transcript remains mounted alongside these controls. This source composition
has Node proof; native batch, full CI, review and staging acceptance remain gates.
