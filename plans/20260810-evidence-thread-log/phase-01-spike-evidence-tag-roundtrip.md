---
phase: 01
title: Spike — unknown crew-evidence tag round-trip
status: planned
priority: P0
effort: S
dependencies: []
---

# Phase 01 — Spike: unknown `crew-evidence` tag round-trip

Gate 1 of `docs/crew/DEVELOPMENT-WORKFLOW.md`. Investigation only. **No
production code.**

## Decision-changing question

Does an unknown, Crew-invented tag on an ordinary message kind survive publish →
relay ingest → storage → query → desktop timeline **unchanged**, and do surfaces
that do not understand it ignore it safely?

If the answer is no, the entire wire design in issue #121 item 2 is invalid and
phases 02-09 must be re-planned around a body convention (like the agent-receipt
JSON payload) instead of a tag. Nothing else in this plan is worth building until
this is settled.

## Why this is not already known

`docs/crew/STATE.md` records that Buzz preserves unknown metadata tags on
**kind 30617** — an addressable repository-announcement event with a different
ingest path. Kind 9 messages carry thread counters, mention fan-out, and edit
overlays. The upstream precedent for a custom tag on kind 9 exists
(`crates/buzz-sdk/src/builders.rs:250` `FAILURE_NOTICE_TAG`) but that tag is
built by the SDK; this slice appends one post-build from the CLI
(`crates/buzz-cli/src/client.rs:590`). The combination is unproven.

## Steps

1. Start a local relay (`just relay`) against a scratch database.
2. Publish a kind-9 message carrying a hand-built `["crew-evidence","test-run"]`
   tag. Use an existing tag-capable path (for example `buzz notes --tag`, or a
   short throwaway script against `buzz-ws-client`) — **do not** add the
   `--evidence` flag yet, that is phase 04.
3. Query the event back (`POST /query` or `buzz messages thread`) and diff the
   returned tag array against what was published.
4. Confirm the tag reaches the desktop timeline model: check that
   `formatTimelineMessages` (`desktop/…/lib/formatTimelineMessages.ts:520`)
   leaves it on `TimelineMessage.tags`, in the running app or via a unit probe.
5. Regression check: publish a normal kind-9 message in the same thread and
   confirm reply counters, mentions and rendering are unaffected.
6. Ignore-safety check: confirm the mobile Flutter client and the web client
   render the tagged message as an ordinary message rather than erroring.
7. Record whether an edit of a tagged message preserves the tag
   (`applyEditTagOverlay` behavior) — this decides whether an edited evidence
   report keeps its card.

## Deliverable

`docs/crew/spikes/0015-evidence-tag-roundtrip.md` following
`docs/crew/templates/SPIKE.md`, ending with **`PASS`**, **`FAIL`**, or
**`INCONCLUSIVE`**. Next free spike id is 0015 (`docs/crew/spikes/` currently
ends at `0014-agent-attention-recovery.md`).

## Acceptance criteria

- The spike answers the question above with commands and observed output, not
  reasoning from the code.
- Steps 3, 4 and 5 each have a recorded observation.
- The verdict line is one of `PASS` / `FAIL` / `INCONCLUSIVE`.

## Exit conditions

- **PASS** → phases 02 and 03 unblock.
- **INCONCLUSIVE** → narrow the question and re-spike; do not proceed on hope.
- **FAIL** → stop the plan and return to the founder with the alternative
  (evidence as a structured body payload, like `KIND_AGENT_RECEIPT`), which
  changes the CLI phase, the desktop parser, and the DECISIONS entry.

## Risk

Low. Read-only against a scratch relay; no repo files change except the new
spike document.
