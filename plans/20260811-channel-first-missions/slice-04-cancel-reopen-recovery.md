---
phase: 04
title: Cancel, reopen, retry, and recovery
status: draft
priority: P1
effort: L
dependencies: ["03"]
---

# Slice 04 — Cancel, reopen, retry, and recovery

This slice is not started. It adds explicit human transitions only after
promotion, live input, and review have passed their founder gates.

## Question and decision

How can a founder stop and later resume Mission work without inferring intent
from a worktree, receipt, or expired local cache? The marker transition remains
thread-scoped and must use the approved promotion wire decision; no new
authoritative local store is permitted.

## Verified seams

* Thread root/reply parsing:
  `crates/buzz-acp/src/queue.rs:1082-1130` and
  `crates/buzz-sdk/src/builders.rs:179-190`.
* Worktree/session provisioning is fail-closed:
  `crates/buzz-acp/src/pool.rs:3019-3078` and
  `crates/buzz-acp/src/thread_workspace.rs:244-270`.
* Active-turn control state is local:
  `desktop/src/features/agents/activeAgentTurnsStore.ts:83-106`.
* Durable ACP input recovery is at
  `crates/buzz-acp/src/elicitation.rs:673-710,1357-1400`.
* Mission phase derivation is at
  `desktop/src/features/messages/lib/projectThreadMissionControl.ts:30-91`.

## RED contracts

* `cancel_requires_valid_owner_marker` — unrelated author/channel/root markers
  do not cancel a Mission.
* `cancel_stops_live_work_without_erasing_history` — explicit stop clears live
  control state while durable receipts/questions remain readable.
* `reopen_after_cancel_restores_planned` — a valid reopen creates a new active
  Mission state without rewriting prior events.
* `duplicate_and_out_of_order_transitions_are_stable` — permutations resolve
  deterministically by event timestamp and event ID.
* `reconnect_replays_transitions` — cancel/reopen state reconstructs after
  relay reconnect with all local stores empty.
* `retry_after_failure_preserves_causality` — a retry does not delete the
  failed receipt or attach to an unrelated root.

## Implementation steps

1. Spike and pin transition tag parsing and authorization fixtures.
2. Add RED tests for duplicate, stale, unrelated, and reconnect arrivals.
3. Reuse existing exact conversation/turn control targets for explicit stop;
   do not invent a second session identity.
4. Extend the pure projection to derive cancellation/reopen/retry state.
5. Verify Project worktree behavior remains governed by trusted metadata and
   does not broaden ordinary-channel provisioning.

## Gate — observable founder outcome

The founder promotes work, starts a turn, explicitly stops it, sees the
durable Mission remain with its prior history, reopens it, retries after a
failure, and can reconnect/restart without losing the transition state.

## Risks and rollback

The highest risk is conflating explicit stop with deletion or silently
re-provisioning an untrusted workspace. Keep cancellation additive, preserve
all durable history, and fail closed on workspace/session mismatch. Rollback
removes transition rendering and control wiring; prior marker messages remain
ordinary readable thread messages.

## Definition of Done

All transition RED contracts are green; explicit stop/reopen/retry behavior is
root-scoped and owner-authorized; reconnect reconstruction passes; no local
TTL store is an authority; the founder recovery gate passes.
