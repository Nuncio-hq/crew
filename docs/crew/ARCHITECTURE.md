# Crew Architecture

## Relationship to Buzz

Crew keeps Buzz's relay, Nostr identity model, desktop shell, channels, and ACP
harness. Crew adds a manager-facing orchestration layer.

```text
Manager
  |
  v
Crew office surfaces: channels/threads, Focus (Workbench), Tool Pane,
Projects/worktrees, Hermes hire (React/TypeScript on the Buzz shell)
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

Today `buzz-acp` captures one process working directory and uses it for new
sessions. The first Crew phase does not change `session/new.cwd`.

Instead, Project-channel context carries the absolute workspace path. Agents
must target that path explicitly. The verified provider behavior is:

| Provider    | Absolute Project path with shared session cwd                         |
| ----------- | --------------------------------------------------------------------- |
| Codex       | Works through `buzz-dev-mcp`; native workspace write alone is blocked |
| Claude Code | Works through ACP in `bypassPermissions`                              |
| Cursor      | Works through native ACP agent mode                                   |
| Devin       | Works through native ACP with an approved write                       |

This proves feasibility but does not make the Project the provider's semantic
root. Automatic discovery of repository-local instructions, relative paths, or
tools that assume process cwd may still be incomplete.

Escalation paths:

1. If only writable scope is missing, spike ACP `additionalDirectories`.
2. If repository-root semantics are required, spike per-Project
   `session/new.cwd`.
3. A Rust change requires evidence that context plus absolute paths is
   insufficient and explicit approval of its upstream maintenance cost.

## Relay event model (formerly "board", superseded by D-037)

Crew state must be reconstructible from relay events after restart or on a
second client. React may cache a projection but cannot own state.

Minimum event semantics to spike:

- card identity and Project reference;
- current column;
- transition author and timestamp;
- assignment through agent mentions;
- priority and queue ordering;
- input request and resolution;
- completion and reopening;
- idempotency and duplicate delivery;
- deterministic conflict resolution for concurrent transitions.

The final kind and tag schema remain undecided until a kind-collision audit and
round-trip compatibility spike pass.

## Capacity invariant

`Working <= 3` is an orchestration invariant. A transition into `Need Input`
must atomically release a working slot from the board's projected state. The
relay event stream remains the source; clients must reach the same projection
despite reconnects, duplicate events, or out-of-order delivery.

## Session lifecycle

A meeting is a resumable session:

- create when work begins;
- persist enough identity to resume;
- stop provider processes when no turn is active;
- restore context when a later mention or manager response arrives;
- never require a process to remain blocked while waiting for a person.

Session persistence and board/card persistence are related but separate. A card
must remain understandable even if a provider session cannot be resumed.

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
