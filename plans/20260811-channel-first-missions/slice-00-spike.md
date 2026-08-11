---
phase: 00
title: Spike — channel-first mission reality check
status: draft
priority: P0
effort: M
dependencies: []
---

# Slice 00 — Spike: channel-first mission reality check

This slice is a pre-implementation spike. **No question below has been run by
this PR.** The predictions are hypotheses from the baseline plan, not results.
Each result must be recorded with the exact command output and fixture/event
IDs before Slice 01 starts.

## Q1 — unknown mission tag round-trip

### Question and decision changed

Can an unknown `["crew-mission", "promote"]` tag on a kind-9 event survive
publish, relay storage, cold query, reconnect replay, and desktop timeline
ingestion unchanged? A failure blocks Slice 01 and requires the settled D-1
wire shape to be re-planned around a receipt-style body convention. The
adjacent evidence-tag spike is the precedent (`origin/devin/1786360062-evidence-thread-log`,
`crates/buzz-cli/src/commands/evidence.rs:5-34`,
`desktop/src/features/messages/lib/evidenceTag.ts:1-19`).

### Smallest realistic environment

Use the repository's disposable local relay/Postgres setup, one generated
Nostr identity, one open NIP-29 channel, and the desktop mock bridge only for
the final timeline-model assertion. Do not start an ACP agent.

### Reproducible method

1. Activate Hermit and ensure the existing local services:

   ```text
   . ./bin/activate-hermit
   just _ensure-services
   just _ensure-migrations
   ```

2. Start the local relay with the repository's normal development command:

   ```text
   just relay
   ```

   Use the authentication settings from the evidence-tag spike.
3. Publish one kind-9 event with:

   ```text
   ["h", "<channel-id>"],
   ["crew-mission", "promote"],
   ["e", "<root-id>", "", "reply"]
   ```

   Use a disposable Rust publisher or the existing relay test client; do not
   add a repository fixture.
4. Query the event through `POST /query`, close the publisher, reconnect with a
   fresh client, and query the same event again.
5. Compare the tag arrays byte-for-byte. Then inject the returned event into
   the existing desktop model through `desktop/src/testing/e2eBridge.ts` and
   assert that raw tags remain available, matching the existing timeline
   preservation path documented by the evidence-tag spike.

### Pre-declared criteria

* **PASS:** both cold query and reconnect replay return the exact tag pairs,
  with the normal `h` and NIP-10 tags intact; desktop exposes the raw tag.
* **FAIL:** relay rejects, strips, rewrites, or desktop drops the unknown tag.
* **INCONCLUSIVE:** relay or desktop cannot be started, or the result depends
  on an unrecorded fixture/setup failure.

### Plan on FAIL

Do not write Slice 01 production code. Escalate D-1 and re-plan promotion as a
receipt-style body convention, preserving normal thread rendering.

## Q2 — ordinary-channel worktree feasibility

### Question and decision changed

Can a promoted thread in an ordinary, non-Project channel obtain an isolated
worktree using today's ACP path? A PASS would be an unexpected baseline change
requiring founder review. A FAIL confirms the settled D-3 split: promotion
anywhere, worktree only where trusted Project metadata exists.

### Smallest realistic environment

Use a disposable Git repository with a clean base checkout, one ordinary
channel, one root/reply pair, and the existing `buzz-acp` thread-workspace
test seams. No Project announcement or `buzz://project-workspace` context may
be seeded.

### Reproducible method

1. Create a temporary clean Git repository and record its absolute path.
2. Create an ordinary channel root and reply with canonical `h`/`e` tags.
3. Invoke the same ACP prompt/workspace resolution path exercised by
   `crates/buzz-acp/src/pool.rs:3019-3078`.
4. Capture whether `parse_project_workspace` and worktree planning are reached
   without trusted Project metadata. If the harness requires a test seam,
   add no code; use the existing `thread_workspace.rs` unit-test fixture or a
   disposable test invocation.
5. Record whether a `.buzz-worktrees` checkout is created and whether the
   root claim verifies.

### Pre-declared criteria

* **PASS:** a clean isolated worktree is created and root-verified without
  Project workspace metadata.
* **FAIL (expected prediction, not a result):** resolution returns no trusted
  workspace or fails closed before creation. The prediction is based on
  `crates/buzz-acp/src/pool.rs:3019-3078`,
  `crates/buzz-acp/src/thread_workspace.rs:244-270`, and the owner mismatch
  error at `crates/buzz-acp/src/thread_workspace.rs:141-148`.
* **INCONCLUSIVE:** the ACP harness cannot be exercised without adding
  production code or without a valid disposable repository.

### Plan on FAIL

Do not broaden worktree provisioning in Slice 01. Continue with durable Mission
state and the required plain-language no-worktree explanation outside a trusted
Project context.

## Q3 — receipt availability in the real configuration

### Question and decision changed

Does the founder's actual ACP configuration publish a kind-46043 receipt after
a completed turn? A PASS unblocks Slice 03's review projection. A FAIL means
receipt-dependent work needs explicit configuration or scope treatment before
UI work.

### Smallest realistic environment

Use one configured managed agent, one owner-authored thread with a valid
mention, the founder's normal relay, and one short successful ACP turn. Do not
use a synthetic receipt as evidence.

### Reproducible method

1. Record the effective ACP configuration without exposing secrets.
2. Confirm whether `agent_receipts_enabled` is enabled; the baseline default is
   `crates/buzz-acp/src/config.rs:1495`.
3. Run one short completed turn through the normal ACP path.
4. Query the relay for kind `46043`, scoped by the thread's `h` and canonical
   `e` tags. Reconnect with a fresh client and query again.
5. Record the event ID, kind, tags, and whether it is consumed by the desktop
   receipt projection at `desktop/src/features/agents/agentReceiptStore.ts:182-244`.

### Pre-declared criteria

* **PASS:** a signed 46043 appears on the relay and is available after
  reconnect.
* **FAIL (expected prediction, not a result):** no receipt appears because
  `agent_receipts_enabled` defaults to false; this prediction is based on
  `crates/buzz-acp/src/config.rs:1495` and publication gating at
  `crates/buzz-acp/src/pool.rs:5544-5553`.
* **INCONCLUSIVE:** the configured agent cannot complete a turn or relay
  access is unavailable.

### Plan on FAIL

Do not claim `ready_for_review` is reachable. Escalate the configuration
decision and block Slice 03 until receipt publication is available or the
slice is explicitly narrowed.

## Q4 — owner identity for ordinary-channel promotion

### Question and decision changed

Does the owner pubkey available in an ordinary channel resolve reliably enough
to authorize a promotion using the same ownership pattern as receipt review?
The result blocks Slice 01's owner-authorization implementation if the
community owner is absent or ambiguous; it does not change the settled D-1
wire shape.

### Smallest realistic environment

Use the desktop mock bridge, one community owner, one non-owner, one ordinary
channel, one root/reply pair, and the existing owner-gated receipt rendering
fixture. Do not publish a Mission marker.

### Reproducible method

1. Seed the owner and non-owner identities through
   `desktop/tests/helpers/bridge.ts:911-930`.
2. Seed an ordinary channel and root/reply event with canonical `h`/`e` tags.
3. Exercise the same profile/owner resolution used by
   `desktop/src/features/messages/ui/AgentReceiptMessageBody.tsx:13-65` and
   the reaction ownership path at `desktop/src/features/messages/ui/MessageRow.tsx:407-408`.
4. Repeat with the owner omitted from the channel roster while retaining the
   community identity. Record whether authorization is deterministic.

### Pre-declared criteria

* **PASS:** exactly one owner identity resolves in both normal and reconnect
  cases, and non-owner identity is rejected.
* **FAIL:** owner cannot be resolved, multiple owners resolve, or authorization
  depends on a local cache.
* **INCONCLUSIVE:** the mock bridge cannot represent the ordinary-channel
  ownership case without changing production code.

### Plan on FAIL

Do not use community-owner authorization by assumption. Stop Slice 01 and
return the authorization implementation to the founder; do not change the
settled D-1 wire shape in the slice.

## Gate

All four questions have a recorded PASS, FAIL, or INCONCLUSIVE result with
commands, fixtures, and captured evidence. Q2 and Q3 remain expected-FAIL
predictions until actually run; they are not results. No production
implementation begins on an INCONCLUSIVE question.
