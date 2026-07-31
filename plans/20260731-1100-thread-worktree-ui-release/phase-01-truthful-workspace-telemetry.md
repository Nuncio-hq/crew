# Phase 01 — Truthful Workspace Telemetry

## Overview

Priority: high. Status: complete.

Expose the existing deterministic worktree result to Desktop without adding a
new HTTP surface or publishing filesystem paths as normal channel messages.

## Architecture

`buzz-acp` emits owner-scoped encrypted observer frames after worktree
verification:

- `thread_workspace_ready`: root event ID, branch, path, name, base revision.
- `thread_workspace_error`: root event ID and safe error text.

Desktop projects these frames into a root-event keyed external store. A thread
root containing trusted Project workspace metadata supplies the initial
`preparing` state; telemetry is authoritative for `ready` or `error`.

## Related Files

- `crates/buzz-acp/src/thread_workspace.rs`
- `crates/buzz-acp/src/pool.rs`
- `crates/buzz-acp/src/thread_workspace_tests.rs`
- `desktop/src/features/agents/observerRelayStore.ts`
- new focused projection helper/tests under `desktop/src/features/agents/`

## Implementation

1. Return structured metadata from worktree provisioning.
2. Emit ready/error frames with existing channel/conversation/turn context.
3. Parse, validate, and cache only well-formed workspace payloads.
4. Reset the projection through the existing observer-store community reset.
5. Cover idempotent reuse, concurrent roots, malformed payloads, and errors.

## Success Criteria

- Different roots never share projection state.
- Ready is emitted only after Git verification succeeds.
- No filesystem path appears in visible message content or relay plaintext.
- Existing non-Project and reused-session behavior is unchanged.

## Risks

- Observer loss can leave a thread in preparing state. The UI must label that
  as awaiting confirmation, not infer success.
- Worktree paths are sensitive local metadata; keep frames encrypted and
  owner-scoped.

## Unresolved Questions

None.
