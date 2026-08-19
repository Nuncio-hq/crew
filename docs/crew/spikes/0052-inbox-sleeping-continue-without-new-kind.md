# Spike 0052 — Inbox can show sleeping sessions without a new kind

- **Status:** PASS
- **Date:** 2026-08-19
- **Founder ask:** see stuck sessions and Continue them (Cursor-like recovery)

## Question

Can Mission Inbox list **sleeping / dead / needs-you** sessions and offer
Wake / Continue using telemetry Crew already has, without a new event kind
or a React-owned session registry?

## Decision affected

Whether the first Continue slice is a desktop projection change (Inbox +
desk) or a new Buzz/Crew protocol.

## Hypothesis

The harness already names sleep (Listening) and wake-on-mention. Inbox
already *collects* sleeping pubkeys, then **drops** those rows. Failed and
lost-contact already land in Needs attention. No new kind is required.

## Scope

- Code read on `c795b02add75ec3c14967d6de232d2084c283ec8` (`main`)
- Files:
  - `desktop/src/features/home/lib/missionInbox.ts`
  - `desktop/src/features/home/useMissionInboxSections.ts`
  - `desktop/src/features/home/ui/MissionInboxSections.tsx`
  - `desktop/src/features/agents/managedAgentRuntimeStatus.ts`
  - `crates/buzz-acp/src/pool_lifecycle.rs`
- Time: one code-read pass. No live agent, no UI change.

## Exclusions

- Mobile Continue
- CoS auto-delegate (spike 0053)
- New ACP methods
- Proving every engine can `session/load` (already spike 0022)
- Production Inbox change

## Pass criteria

1. Sleeping pubkeys are already computed from managed-runtime status.
2. Those pubkeys are used to **exclude** rows from Inbox today.
3. A documented wake path exists that is scoped to work arriving for that
   agent (mention / inbound turn), not a sibling thread’s session id.
4. Failed / lost-contact already have Inbox rows and an Inspect/Respond
   action (so Continue is an action rename + wake, not a new store).

## Fail criteria

Sleeping / dead cannot be distinguished from “no work” without a new
durable event, or wake cannot be aimed at one thread’s session.

## Environment

- Commit: `c795b02add75ec3c14967d6de232d2084c283ec8`
- OS: Linux cloud agent
- Auth: none (read-only)

## Method

Read the Inbox derivation and the ACP pool state machine. Quote the
exclude + wake seams. Do not run the desktop app.

## Results

1. `useMissionInboxSections` builds `sleepingAgentPubkeys` from
   `isManagedAgentRuntimeSleeping` (`useMissionInboxSections.ts:93-106`).
2. `deriveMissionInboxSections` skips outcome rows when the agent is
   sleeping (`missionInbox.ts:229-236`) and skips active turns when every
   participant is sleeping (`missionInbox.ts:339-342`). Sleeping work
   therefore **vanishes** from Inbox.
3. `MissionInboxState` has `needsYou | failed | lostContact |
   telemetryUnavailable | possiblyStalled | readyToReview | working` — no
   `sleeping` (`missionInbox.ts:32-39`).
4. Exception rows offer **Respond** or **Inspect**, not Wake/Continue
   (`MissionInboxSections.tsx:119-126`).
5. Agent card copy is already `Sleeping · wakes on mention`
   (`managedAgentRuntimeStatus.ts:10`). ACP pool: Listening + inbound work
   → Waking (`pool_lifecycle.rs` header comment). Spike 0022: wake must
   load **that** thread’s session id, never a sibling.

## Edge cases observed

- Needs-you for a sleeping agent is **not** dropped in the first
  needs-you loop (only later outcome/turn loops). A blocking question can
  still appear; idle sleeping work cannot.
- Mission Inbox excludes Sleeping on purpose today (`STATE.md` #169).
  That rule is the bug relative to the founder Continue ask, not a
  protocol hole.
- `ARCHITECTURE.md` still mentions `Working <= 3` board caps. D-037
  superseded board-as-home. Do not revive columns here.

## Limitations

- Did not run a live Hermes/Codex sleep → mention → resume.
- “Process died” vs “Listening sleep” still needs a named harness
  outcome if copy would otherwise lie. Missing folder already has
  `Missing` (#217).
- Continue that **rebuilds** a session when `session/load` fails must
  stay fail-closed (D-048 / D-049). This spike does not re-prove that.

## Verdict

**PASS.** First Continue slice is an Inbox projection: stop dropping
sleeping rows, add an honest `sleeping` (or reuse Needs attention with
Wake copy), and wake via the existing mention path for that thread.
No new event kind.

## Follow-up test contract

RED before implementation:

1. Fixture: one sleeping agent + one active turn on thread A → Inbox
   shows a sleeping/Wake row for A.
2. Fixture: sleeping A + working B → Continue/Wake on A does not include
   B’s session id.
3. Needs-you still outranks sleeping on the same conversation.

## Cleanup

None. Read-only.
