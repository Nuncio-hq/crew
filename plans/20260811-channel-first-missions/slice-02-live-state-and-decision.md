---
phase: 02
title: Live state and inline human decision
status: draft
priority: P0
effort: L
dependencies: ["01"]
---

# Slice 02 — Live state and the decision, in place

This slice is not started. It depends on a passing restart gate from Slice 01.

## Question and decision

How does a durable Mission strip expose current work without promoting
observer telemetry into durable truth? The existing durable user-input
pipeline remains authoritative for `needs_input`; active turns may provide
only a transient `working` hint.

## Verified seams

* Durable question publication and recovery:
  `crates/buzz-acp/src/elicitation.rs:452-480,853-956,1027-1046`.
* Desktop input parsing and projection:
  `desktop/src/features/channels/lib/userInput.ts:150-322` and
  `desktop/src/features/agents/userInputAttentionProjection.ts:286-403`.
* Active-turn state is in-memory:
  `desktop/src/features/agents/activeAgentTurnsStore.ts:83-106`.
* Existing thread phase derivation:
  `desktop/src/features/messages/lib/projectThreadMissionControl.ts:30-91`.
* Needs-you is TTL-bound local state:
  `desktop/src/features/agents/needsYouStore.ts:12,35-50`.

## RED contracts

* `unresolved_46040_beats_working_hint` — a durable unanswered 46040 projects
  `needs_input` even when an active-turn frame is present; it fails because no
  Mission strip consumes these inputs.
* `resolved_46042_removes_needs_input` — matching resolution removes the
  attention state; it fails because Mission state has no durable input reducer.
* `working_is_not_persisted` — clearing active-turn state after restart removes
  only the working hint while leaving the promoted Mission; it fails because
  there is no Mission UI/projection.
* `inline_answer_targets_same_request` — an inline answer publishes the
  existing 46041 relation and the same ACP session resumes; it fails because
  the Mission strip has no inline decision action.
* `question_survives_restart` — relay replay restores the question with local
  stores empty; it fails until the strip consumes the existing durable input
  projection.

## Implementation steps

1. Add fixtures for request, answer, and resolved events using canonical
   `h`/`e`/`p` validation.
2. Extend the pure Mission projection with durable input transitions and an
   optional active-turn display hint.
3. Render the existing input question inline without copying it into a new
   Mission-owned store.
4. Wire answer submission to the existing ACP user-input path.

## Gate — observable founder outcome

An agent turn pauses for an answer; the founder quits and relaunches; the
question is still visible in the same thread; the founder answers inline; the
same ACP session resumes. If the question depends on a TTL-bound local store,
the slice fails.

## Risks and rollback

The main risk is treating active observer telemetry or needs-you cache state as
durable. Keep those inputs display-only and derive attention from 46040/46042.
Rollback removes the strip extension while leaving existing ACP input events
and channel rendering intact.

## Definition of Done

RED tests are green; the question and answer use existing durable event paths;
working is explicitly ephemeral; restart/replay preserves unresolved input; the
founder gate passes.
