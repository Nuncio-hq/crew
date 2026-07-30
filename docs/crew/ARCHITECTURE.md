# Crew Architecture

## Relationship to Buzz

Crew keeps Buzz's relay, Nostr identity model, desktop shell, channels, and ACP
harness. Crew adds a manager-facing orchestration layer.

```text
Manager
  |
  v
Crew board and card detail (new React/TypeScript)
  |
  v
Buzz relay (signed events; shared state)
  |
  +--> channels and card conversations
  +--> project and board events
  +--> agent mentions
  |
  v
buzz-acp -> provider ACP adapter -> coding agent
                                  |
                                  v
                         local filesystem tools
```

The relay is the shared coordination log. It is not the source-code store.

## Three data planes

| Data                                 | Authority         | May leave the machine through |
| ------------------------------------ | ----------------- | ----------------------------- |
| Board, messages, status, assignments | Local Buzz relay  | Relay WebSocket               |
| Working directory and source code    | Local filesystem  | Nothing by default            |
| Images, video, and large artifacts   | Local media store | Uploaded media URL            |

Do not put source trees into relay events or media uploads. Do not make React
state authoritative for board state.

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

## Board event model

Board state must be reconstructible from relay events after restart or on a
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
