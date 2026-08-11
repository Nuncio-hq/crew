# Spike 0021 — Unknown `crew-evidence` tag round-trip

- **Status:** PASS
- **Date:** 2026-08-10

## Question

Does an unknown `["crew-evidence", "test-run"]` tag on an ordinary kind-9
message survive publish, relay ingest, storage, query, and the desktop
timeline model unchanged, while clients that do not understand it continue to
render the message normally?

## Decision affected

This gates the issue #121 wire design. A failed round-trip would require
replacing the tag convention with a structured message-body convention before
the CLI or desktop phases begin.

## Hypothesis

The relay stores arbitrary tags on kind-9 events, and the desktop timeline
already carries raw tags through to `TimelineMessage.tags`. Mobile and web
should ignore the unknown tag because their ordinary message models use event
kind/content/thread tags rather than a closed tag enum.

## Scope

- **Providers/components:** local Rust relay, PostgreSQL storage, desktop
  timeline formatter and edit overlay, mobile model/read path, web event model.
- **Files or systems:** `crates/buzz-relay`, local PostgreSQL, desktop
  `formatTimelineMessages` and `applyEditTagOverlay`, mobile `NostrEvent` /
  `UserNote`, web event consumers.
- **Time or attempt boundary:** one local relay run and one disposable
  publish/query fixture on 2026-08-10.

## Exclusions

- No production code or CLI `--evidence` flag was added.
- No mobile or web app was launched; ignore-safety was checked by reading the
  real model and rendering paths, which is acceptable for this phase where
  those clients are impractical to run.
- This does not prove that future clients will preserve the tag when editing;
  it checks the current desktop edit overlay.
- It does not prove evidence authenticity or validate the eventual evidence
  enum.

## Pass criteria

- The relay accepts the tagged kind-9 event.
- `POST /query` returns the exact `["crew-evidence","test-run"]` pair.
- The desktop formatter exposes that pair on `TimelineMessage.tags`.
- A normal reply in the same thread is accepted, persisted, and rendered.
- Mobile and web have no closed-tag failure path for the unknown pair.
- Editing preserves the unknown non-`imeta` tag.

## Fail criteria

- The relay rejects the event because of the unknown tag, strips or rewrites
  that pair, or the desktop model drops it.
- A normal same-thread message fails or the unknown tag causes a client error.

## Environment

- **Commit:** `35af7401929261e34b724b7548daca3792c2946b` (`fix(agent): harden attention recovery (#114)`)
- **OS:** Ubuntu Linux
- **Tool/provider versions:** Hermit toolchain; Rust relay and PostgreSQL
  services from the repository's Docker Compose setup; desktop Node test loader.
- **Authentication class, without secrets:** disposable generated Nostr key,
  local relay NIP-42 WebSocket auth, and development HTTP `X-Pubkey` bridge
  mode (`BUZZ_REQUIRE_AUTH_TOKEN=false`).

## Method

1. Started the repository services and migrations:

   ```text
   . ./bin/activate-hermit && just _ensure-services
   . ./bin/activate-hermit && just _ensure-migrations
   ```

   Observed: `Container buzz-postgres Healthy`, `Container buzz-minio Healthy`,
   `Services already healthy`, `Database migrations complete`, and local
   community seed rows.

2. Started a local relay against the Docker PostgreSQL database:

   ```text
   . ./bin/activate-hermit && BUZZ_REQUIRE_AUTH_TOKEN=false \
     BUZZ_BIND_ADDR=127.0.0.1:3000 cargo run -p buzz-relay
   ```

   Observed: `Postgres connected` and `buzz-relay TCP listening` on
   `127.0.0.1:3000`.

3. Used a disposable Rust program in `/tmp/crew-spike121/` (not added to the
   repository) to create an open stream channel, publish a kind-9 event with
   hand-built tags
   `[["h","<channel>"],["crew-evidence","test-run"]]`, then publish a normal
   kind-9 reply with an `e` reply tag. The program queried both events using
   `POST /query` and recorded the JSON response in
   `/tmp/spike121-roundtrip.json`.

4. Ran the desktop unit probe from the repository's real test loader:

   ```text
   . ./bin/activate-hermit && node --import ./desktop/test-loader.mjs \
     --experimental-strip-types --test /tmp/spike121.test.mjs
   ```

5. Read the mobile and web paths for unknown-tag handling. No mobile/web
   process was launched.

## Results

### Step 3 — publish, storage, query, and tag-array diff

Command:

```text
. ./bin/activate-hermit && cargo run --manifest-path \
  /tmp/crew-spike121/Cargo.toml > /tmp/spike121-roundtrip.json
```

Observed publish output:

```json
{
  "channel_create_status": 200,
  "channel_create": {"accepted": true, "message": ""},
  "tagged_publish": {"accepted": true, "message": ""},
  "normal_publish": {"accepted": true, "message": ""},
  "query_status": 200
}
```

The tagged event was returned by `/query` as kind 9 with this exact tag array:

```json
[
  ["h", "05ec2cec-d38b-4f88-93be-34e35259a700"],
  ["crew-evidence", "test-run"]
]
```

The fixture's input tag array was byte-for-byte equivalent:

```json
[
  ["h", "05ec2cec-d38b-4f88-93be-34e35259a700"],
  ["crew-evidence", "test-run"]
]
```

This proves the unknown pair survived relay ingest, PostgreSQL storage, and
HTTP query unchanged. The relay's event log also recorded the authenticated
connection and both accepted publishes.

### Step 4 — desktop timeline model

The probe output was:

```text
timeline tags: [["h","05ec2cec-d38b-4f88-93be-34e35259a700"],["crew-evidence","test-run"]]
✔ formatTimelineMessages preserves unknown tag on TimelineMessage.tags
```

The production path at
`desktop/src/features/messages/lib/formatTimelineMessages.ts:520-531` calls
`applyEditTagOverlay` and returns the result as `tags`; it does not filter
unknown tag names.

### Step 5 — same-thread regression

The normal kind-9 reply was accepted and queried back with its reply reference:

```json
{
  "content": "spike-121 normal reply",
  "kind": 9,
  "tags": [
    ["h", "05ec2cec-d38b-4f88-93be-34e35259a700"],
    ["e", "95327ceadd7b1bc3c6cdd52d7561d23d1496c886921e39ae5bceaf59b9bfcedb", "", "reply"]
  ]
}
```

Direct PostgreSQL inspection of `thread_metadata` showed the root's counters:

```text
95327ceadd7b1bc3c6cdd52d7561d23d1496c886921e39ae5bceaf59b9bfcedb|1|1
d7d8e62d3ed80a221d3a0124c65a9d10af9bb8dfe53b8894b7d83777b32a7413|0|0|95327ceadd7b1bc3c6cdd52d7561d23d1496c886921e39ae5bceaf59b9bfcedb
```

The desktop probe rendered both messages:

```text
normal timeline rows: 2 bodies: [ 'spike-121 tagged', 'spike-121 normal' ]
✔ normal message in same thread remains ordinary
```

The fixture attempted a self `p` mention. The relay omitted that self
mention from the queried event, independently of the evidence tag. This is a
relay normalization detail, not a tag-round-trip failure; the same-thread
reply and counters remained healthy.

### Step 6 — ignore safety

This was code-read rather than a running mobile/web check:

- Mobile `mobile/lib/shared/relay/nostr_models.dart:95-136` deserializes all
  event tags into `List<List<String>>` without an allow-list.
- Mobile `mobile/lib/features/pulse/pulse_models.dart:17-45` projects tags
  only for known reply/mention helpers, while `NoteCard` renders the event
  content normally.
- Web event consumers retain `tags: string[][]` in
  `web/src/shared/lib/nostr-signer.ts:10` and only inspect named tags in
  feature-specific helpers; no generic unknown-tag error path was found.

Therefore an unrecognized `crew-evidence` pair is ignored by these surfaces,
not treated as a parsing error.

### Step 7 — edit overlay

Command:

```text
. ./bin/activate-hermit && node --import ./desktop/test-loader.mjs \
  --experimental-strip-types --test /tmp/spike121.test.mjs
```

Observed:

```text
overlay tags: [["h","05ec2cec-d38b-4f88-93be-34e35259a700"],["crew-evidence","test-run"]]
✔ edit overlay preserves unknown non-imeta tag
```

`desktop/src/features/messages/lib/applyEditTagOverlay.mjs:31-44` preserves
all original non-`imeta` tags and overlays only `imeta` (plus the special
emoji behavior). Thus a tagged evidence report keeps its tag after an edit.

## Edge cases observed

- The first disposable attempt used a not-yet-created channel and was
  correctly rejected as `restricted: not a channel member`; the fixture was
  corrected to create an open channel first.
- A reply carrying both `root` and `reply` markers was accepted but did not
  create the expected counter row. The NIP-10-compatible single `reply` marker
  produced the expected `reply_count=1` and `descendant_count=1`.
- A self `p` mention was normalized out by the relay. This did not affect the
  evidence tag or thread counters.
- Relay startup emits unrelated MinIO `412 PreconditionFailed` retry warnings
  during its existing object-store conformance probe; startup completed and
  the relay passed its own probe.

## Limitations

- Mobile and web ignore-safety is source-level evidence, not a launched-client
  test.
- The desktop probe used a real production formatter and overlay helper but
  not a full Tauri window.
- Mention preservation was not independently demonstrated because the relay
  normalized the disposable self-mention; the tag and thread behavior were
  directly observed.

## Verdict

**PASS** — the unknown tag survived publish → relay ingest → storage → query
and desktop modeling unchanged; same-thread behavior remained healthy, and
mobile/web code paths ignore unknown tags safely.

## Follow-up test contract

Before implementation, RED tests should assert:

- `messages send --evidence <kind>` emits exactly one validated
  `["crew-evidence", "<kind>"]` tag.
- Desktop renders recognized evidence kinds and falls back to ordinary body
  rendering for unknown values.
- A kind-46043 agent receipt ignores `crew-evidence`.
- Editing a tagged kind-9 message preserves the tag.
- A normal tagged-thread reply keeps relay counters and ordinary rendering.

## Cleanup

- Removed no repository files; no production files were changed.
- Disposable source remains only in `/tmp/crew-spike121/` and
  `/tmp/spike121.test.mjs`; observed JSON remains in
  `/tmp/spike121-roundtrip.json`.
- Docker services and the local relay are still running for other sessions;
  they were not stopped because they are shared development services.
