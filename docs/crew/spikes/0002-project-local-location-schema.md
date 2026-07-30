# Spike 0002 — Project Local Location Schema

- **Status:** PASS with a local-relay boundary
- **Date:** 2026-07-30

## Question

What is the smallest relay representation for a Project's local workspace
location that preserves NIP-34 identity and does not change the meaning of
clone metadata?

## Decision affected

The exact tag contract that RED tests and the first TypeScript Project slice
will implement.

## Hypothesis

A Crew-namespaced tag containing a raw absolute path is smaller and less
ambiguous than either adding a `file://` value to `clone` or storing a file URL
in a separate tag.

## Scope

- kind `30617` repository announcements;
- the existing NIP-34 `d`, `clone`, and `web` semantics;
- Buzz event storage, Project parsing, and metadata-preserving update paths;
- macOS absolute paths for the current single-machine phase;
- JSON round-trip, multiple tags, and replaceable-event identity.

No production code or relay data was changed.

## Exclusions

- Windows drive and UNC paths;
- device identifiers and multi-machine location selection;
- encryption or selective disclosure of event tags;
- changing `session/new.cwd`;
- filesystem access already proved by
  [Spike 0001](0001-project-workspace-absolute-path.md).

## Pass criteria

A candidate passes only if it:

1. leaves Project identity equal to `30617:<pubkey>:<d>`;
2. leaves all Git clone values unchanged;
3. round-trips spaces, Unicode, `%`, and `#` without loss;
4. can coexist with `buzz-channel` and future location types;
5. survives Buzz's existing read-modify-write path;
6. has deterministic invalid and duplicate handling;
7. states its relay privacy boundary honestly.

## Fail criteria

- local path becomes a repository identifier;
- a normal clone consumer receives the local path as Git transport metadata;
- encoding changes the selected path;
- an update silently drops the location;
- duplicate local workspaces are resolved by arbitrary ordering;
- the design implies that a signed plaintext tag is private.

## Environment

- Buzz commit: `63496cc1d4c6f1b7c613801bdcc694169dcf391a`;
- macOS `26.5.2` on Apple silicon;
- Node.js `24.15.0`;
- Cargo `1.95.0`;
- NIP-01 and NIP-34 `master`, retrieved 2026-07-30.

## Standards evidence

[NIP-34](https://github.com/nostr-protocol/nips/blob/master/34.md) defines:

- repository announcement kind `30617`;
- `d` as the repository identifier;
- `clone` values as URLs to give to `git clone`;
- multiple `clone`, `web`, `relays`, and `maintainers` values.

[NIP-01](https://github.com/nostr-protocol/nips/blob/master/01.md) defines
addressable-event identity by kind, author, and `d`. Tags are arrays of strings,
and extension tag names are allowed. Multi-letter extension tags are not
implicitly relay-indexed like single-letter tags.

Therefore a local location can be added without changing identity, but NIP-34
does not define a local-workspace tag.

## Buzz source evidence

- The SDK builds `clone` as transport values and has a separate
  metadata-preserving update builder:
  [`builders.rs`](../../../crates/buzz-sdk/src/builders.rs).
- The repository coordinate is rendered only from kind, owner, and `d` in the
  same builder file.
- The SDK test
  `repo_announcement_with_tags_preserves_metadata_and_canonicalizes_d` proves
  unknown future metadata survives an update.
- The relay's kind `30617` side effect reads the `d` identifier and does not
  reject unrelated tags:
  [`side_effects.rs`](../../../crates/buzz-relay/src/handlers/side_effects.rs).
- The database serializes and reconstructs the complete tag array:
  [`event.rs`](../../../crates/buzz-db/src/event.rs).
- Existing Project creation writes `clone` as a Git location:
  [`useCreateProject.ts`](../../../desktop/src/features/projects/useCreateProject.ts).
- Existing Project identity and clone parsing are separate:
  [`hooks.ts`](../../../desktop/src/features/projects/hooks.ts).

## Candidates

| Candidate | Example | Result | Reason |
| --- | --- | --- | --- |
| Reuse `clone` | `["clone", "file:///Users/me/crew"]` | FAIL | Makes a machine-local locator look like normal Git transport metadata. |
| Extension with file URL | `["buzz-location", "local", "file:///Users/me/crew"]` | Viable, not selected | Preserves clone semantics but adds URI encoding and decoding with no benefit in the current phase. |
| Extension with raw path | `["buzz-location", "local", "/Users/me/crew"]` | PASS | Keeps transport and workspace concepts separate and preserves the picker value exactly. |

## Approved wire contract

```json
[
  ["d", "crew"],
  ["clone", "https://github.com/Nuncio-hq/crew.git"],
  ["buzz-channel", "11111111-1111-4111-8111-111111111111"],
  ["buzz-location", "local", "/Users/me/Code/crew"]
]
```

The new record shape is:

```text
["buzz-location", <location-type>, <type-specific-locator>]
```

For the first phase:

- the only interpreted location type is `local`;
- the locator is a raw absolute macOS path, not a `file://` URL;
- a Project update replaces existing `buzz-location/local` records with the
  newly selected path and preserves durable tags; native Buzz's transient
  setup-only `auth` tag is stripped;
- serialization does not trim, normalize, resolve symlinks, or percent-decode
  the path;
- activation checks availability separately from wire parsing;
- unsupported location types and trailing fields are preserved, not interpreted.

This keeps the schema extensible without inventing device identity before it is
needed.

## Validation rules

For `local`, reject:

- a missing or empty locator;
- a relative path;
- a `file://` locator;
- a NUL character;
- more than one active `local` record in the current single-machine phase.

A duplicate local record is ambiguous. Workspace actions must stop and surface
the Project as needing correction instead of choosing the first or last tag.

A syntactically valid path that is currently missing or inaccessible remains
valid relay metadata. After restart the UI reports it as `not locally
verified`; an agent or tool surfaces a missing or permission error when it
uses the path. Crew must not silently erase or rewrite the event.

## Method

### Existing Buzz contract

The following targeted test was run in the pinned Hermit environment:

```text
cargo test -p buzz-sdk \
  repo_announcement_with_tags_preserves_metadata_and_canonicalizes_d \
  -- --nocapture
```

Result: one test passed, zero failed.

### Disposable schema fixture

A temporary Node.js fixture created the same kind `30617` announcement under
all three candidates and asserted:

- JSON serialization round-trips every tag;
- identity remains `30617:<owner>:crew`;
- a metadata-preserving update keeps every candidate tag;
- clone parsing remains remote-only for extension candidates;
- two repeated extension records retain order and exact values;
- invalid local paths are rejected by the proposed validation contract.

Fixture paths included spaces, Vietnamese text, an emoji, a literal `%20`,
another literal `%`, and `#`.

## Results

| Observation | Result |
| --- | --- |
| Identity before and after location update | Unchanged |
| Raw path JSON round-trip | Exact |
| Existing remote clone value | Unchanged |
| Two repeated `buzz-location` records | Preserved |
| `buzz-channel` and unknown metadata | Preserved |
| Relative, empty, NUL, and `file://` local values | Rejected by contract |
| Existing Buzz future-metadata update test | PASS |

For this selected path:

```text
/Users/oscar/Code/Space Name/đội%20x/#draft
```

the file URL candidate became:

```text
file:///Users/oscar/Code/Space%20Name/%C4%91%E1%BB%99i%2520x/%23draft
```

It decoded correctly with a matching URL library, but the wire value no longer
matched the file picker value and required another conversion boundary. The raw
path needed no conversion.

## Privacy boundary

`buzz-location` is a signed plaintext event tag. Any client allowed to receive
the Project announcement from the relay can read the path. A multi-letter tag
being unindexed by default is not encryption and does not make it secret.

This spike passes only for the accepted current scope: one manager and the
manager's trusted, same-machine relay. The first UI must state that the path is
stored on that relay.

If Crew later connects this Project record to an untrusted relay, adds members,
or replicates it outside the machine, stop and choose a new privacy contract
before publishing local paths. Do not silently treat the present schema as
safe for that expanded scope.

## Edge cases observed

- Literal percent sequences become double-encoded inside a file URL.
- A path can be valid metadata while its volume is detached.
- Resolving symlinks during serialization would unexpectedly change the
  manager-selected location.
- Multiple local records need fail-closed behavior until device selection is a
  real product requirement.
- Existing Buzz update builders can preserve the extension, but the desktop
  Project model does not expose locations yet.

## Limitations

- The fixture did not publish to a live Postgres-backed relay because source
  inspection showed complete tag serialization and the uncertain boundary was
  schema semantics, not relay transport.
- macOS acceptance does not define Windows representation.
- The spike does not solve confidentiality for remote or multi-member relays.
- It does not choose where Project-channel prompt context is assembled.

## Verdict

PASS for:

```text
["buzz-location", "local", "<raw absolute path>"]
```

Use one record per location. In the current single-machine phase, require
exactly one active local record and treat duplicates as manager input.

Do not put local paths in `clone`. Do not use `file://` for the first phase.

## Follow-up RED test contract

Before implementation, tests must fail for the missing behavior:

1. parse one valid `buzz-location/local` record into Project workspace metadata;
2. keep `30617:<pubkey>:<d>` unchanged when the path changes;
3. preserve `clone`, `web`, `buzz-channel`, protection, and unknown tags on
   update;
4. write exactly one local record while preserving unsupported location types;
5. round-trip spaces, Unicode, `%`, `#`, and symlink-shaped paths exactly;
6. reject empty, relative, NUL, and `file://` local values;
7. stop workspace actions on duplicate local records;
8. preserve a missing or inaccessible path as `not locally verified` metadata
   until an agent or tool surfaces the use-time error;
9. include the path only in the matching Project-channel agent context;
10. show that the path is stored on the configured relay;
11. leave `session/new.cwd` unchanged.

The wire contract was approved on 2026-07-30. Production implementation still
requires RED evidence and manager approval of the smallest implementation plan.

## Cleanup

The temporary Node.js fixture was removed. Hermit downloaded its pinned
toolchain and Cargo dependencies into user caches. No production source, relay
event, user Project file, or repository lockfile was changed.

## Implementation follow-up

The later production boundary test closed this spike's relay-transport
limitation. An isolated Postgres-backed Buzz relay accepted the original
announcement, the linked replacement, and a Unicode-path relink. A fresh relay
connection reconstructed the exact signed event and Project-channel context.
See
[`../verification/0001-project-local-workspace.md`](../verification/0001-project-local-workspace.md).
