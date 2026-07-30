# Verification 0001 — Project local workspace

- **Date:** 2026-07-30
- **Result:** PASS
- **Scope:** local path metadata and Project-channel agent context

## Manager-visible outcome

An owned Buzz Project can link or relink one folder through the native macOS
picker. Crew writes the raw absolute path to the existing kind `30617`
announcement and reads it back from the relay. The Project remains the same
`(pubkey, d)` record.

The next explicit agent mention in the Project's canonical channel receives
the newest relay-confirmed absolute path. `session/new.cwd` does not change.

## Boundaries exercised

### Native picker

A disposable unsigned `Buzz.app` used the already registered Tauri dialog
plugin and existing capability. Cancel, spaces, Vietnamese text, CJK text, and
relink passed. Returned values were absolute native paths, not `file://` URLs.

### Real Buzz relay

An isolated local stack used:

- Buzz relay from the approved baseline;
- PostgreSQL 17;
- Redis 7;
- MinIO;
- an ephemeral Nostr keypair and unique Project identifier.

The test:

1. published an initial kind `30617` with `d`, `clone`, `buzz-protect`,
   transient `auth`, and an unknown future tag;
2. linked `/tmp/Nuncio Crew Đồ án`;
3. required relay acknowledgement and fetched the exact signed event id;
4. closed and reopened the WebSocket connection;
5. reconstructed the same Project from relay events;
6. relinked `/tmp/Nuncio Crew 二`;
7. confirmed one canonical local tag, unchanged `(pubkey, d)`, preserved
   durable metadata, stripped `auth`, and `created_at + 1` ordering;
8. resolved the second path into explicit-agent context and rejected the stale
   first path.

Command:

```text
cd desktop
CREW_LIVE_RELAY_URL=ws://127.0.0.1:3000 \
  node --import ./test-loader.mjs --experimental-strip-types \
  --test src/features/projects/project-local-workspace-live-relay.test.mjs
```

Result: `1/1` passed.

The pre-review live-relay-enabled full desktop run passed `3816/3816`. Later
review fixes did not change relay persistence; they added consent, retry, and
render-isolation guardrails.

## Provider evidence

[Spike 0001](../spikes/0001-project-workspace-absolute-path.md) independently
proved Codex, Claude Code, Cursor, and Devin can read and write the supplied
absolute path while ACP session cwd remains elsewhere. Codex must use
`buzz-dev-mcp` for writes outside its native workspace scope.

This implementation did not repeat those provider filesystem probes. It
verifies that the same path now comes from the relay and reaches the outgoing
agent message boundary.

## Normal gates

- Project workspace contracts: pass;
- final full desktop tests: `3824` passed, one gated live-relay test skipped,
  zero failed;
- TypeScript typecheck: pass;
- Biome and repository desktop checks: pass;
- file-size gate: pass;
- production desktop web build: pass;
- Git whitespace check: pass.

The build emitted only the existing large-chunk warnings. Biome emitted two
pre-existing informational notices in a persona catalog relay test.

## Accepted limitations

- The path is plaintext to readers of the selected relay.
- A cold-loaded path is `not locally verified`; there is no restart-time
  filesystem `stat` in this no-Rust slice.
- Missing or denied paths surface when an agent or tool uses them.
- Context is hidden from rendered Markdown but remains in raw copy/edit data.
- Relay lookup failure blocks explicit-agent sends because a multi-character
  `buzz-channel` tag cannot be queried directly.
- Git and worktree management remain out of scope.
- The accepted local path syntax and native picker are macOS/POSIX-only;
  Windows drive and UNC paths are not supported by this slice.
- There is no automated Playwright coverage for the panel and confirmation
  flow. Pure UI policy, rendered-context, native-picker, and relay boundaries
  are tested separately.

## Cleanup

The relay process, isolated containers, Colima VM, unsigned spike app,
disposable worktree, temporary folders, and generated `.env` were stopped or
removed. Existing Buzz Docker volumes were preserved. No user Project or
source workspace was modified by the live test.
