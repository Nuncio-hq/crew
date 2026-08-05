# Retry on error — steering v3

**Status:** implemented, second re-gate passed — one perf fix outstanding before PR
(uncommitted on `buzz/0efbf738ca7b`)

First re-gate (10:32Z) found three CI-failing gate steps, a routing defect, a
hardcoded `prompt_tag`, a dead status branch, and no handler test coverage. All
fixed and independently verified at the second re-gate (10:45Z): full workspace
clippy under `-D warnings`, fmt, both Rust suites, the full desktop suite, tsc
build, and the Tauri crate all green. The one `buzz-acp` failure
(`thread_response_requires_the_exact_requested_root`) was confirmed pre-existing
by running it on a clean `origin/main` checkout.

Outstanding: `FailureNoticeRetryButton` mounts on every `MessageRow` and calls
the polling `useManagedAgentsQuery` before its early return, making every
timeline row a query observer. Split the component so the hook only mounts when
a notice is present.
**Implementer:** Cursor Grok High Fast
**Follows:** [stop + dispatch hold](../260802-1608-agent-stop-and-dispatch-hold/index.md), merged as `9aed9b0cc` (PR #24)

## Outcome

When an agent turn fails, the person can see it is being retried and can re-run
the failed request from the message itself — without retyping it and without
sending a duplicate.

This is the third leg of the steering brief from 2026-08-02 04:48Z. The other
two shipped: mis-send (edit-as-undo, PR #21 + hold, PR #24) and stop (PR #24).
Retry was an explicit Non-goal of the stop plan — "revisit when people are
actually stopping things" — and that condition is now met.

## What exists today

Retry is real but entirely invisible:

| Behaviour | Where |
|---|---|
| Auto-requeue with exponential backoff + ±20% jitter, max 10 attempts | `crates/buzz-acp/src/queue.rs:505`, `MAX_RETRIES` at `queue.rs:30` |
| Auth errors dead-letter immediately (token won't self-repair between attempts) | `crates/buzz-acp/src/lib.rs:3609` |
| On exhaustion, a "⚠️ I couldn't process the last request … Please re-send" notice is posted | `lib.rs:3487` → `pool.rs:4215` |
| Panic path discards the dead-lettered batch **with no notice at all** | `lib.rs:3913` |

Three gaps follow:

1. "Please re-send" *is* the retry UI — the original message is sitting right
   there and nothing can re-run it.
2. Ten backoff attempts span minutes during which the person sees only silence.
   Same failure class PR #24 removed: state the harness has, the UI does not.
3. The panic path loses the message silently.

## The blocker phase 1 exists to remove

`post_failure_notice` builds the notice via
`buzz_sdk::build_message(channel_id, content, thread_ref, &[], false, &[])`
(`crates/buzz-sdk/src/builders.rs:220-238`). The tags produced are `h` plus
thread tags — **no `e` tag to the failed event, no marker tag**.

So a Retry control has nothing to bind to, and the only way for the desktop to
recognise the notice today is to string-match the "⚠️ I couldn't process the
last request" copy. That is the same string-matching-on-agent-output rejected
during edit-as-undo review. Phase 1 is therefore not preparatory tidying — it
is what makes phases 2 and 3 expressible at all.

## Scope

| # | Item | Phase |
|---|---|---|
| a | Failure notice carries a reason class and one `e` per failed event | [phase-01](phase-01-machine-readable-failure-notice.md) |
| b | `retry_turn` control frame re-dispatches the failed batch | [phase-02](phase-02-retry-control-frame.md) |
| c | Retry attempt visible on the activity rail; panic path stops swallowing | [phase-03](phase-03-retry-visibility-and-panic-notice.md) |

Phase 1 blocks both others — the tag shape it settles is what the desktop reads
and what the control frame carries.

## Invariants

1. **Signed events are never mutated.** Retry re-pushes the original signed
   event; the relay stays the authority on edit ownership. Same rule as
   edit-as-undo.
2. **No string-matching on message content** anywhere in the desktop path.
3. **Retry re-dispatches, it does not re-send.** The desktop must never publish
   a fresh kind-9 carrying the same body — that duplicates the person's message,
   moves it in the timeline, and re-notifies every mention in it.
4. **Retry authorization mirrors Stop's**, whatever `useComposerAgentStop` already
   enforces. Do not invent a second rule.
5. Notices stay **kind 9** so every client including mobile renders them; the
   machine-readable part lives in tags.
6. `ChannelPane` stays under the line ratchet; text uses rem tokens, never px.

## Non-goals

- Retry classification per error type (which errors are worth retrying at all).
  The existing auth-vs-rest split is enough until someone asks for more.
- Side-effect ledger. Re-running a turn that already had partial effects is a
  real problem and not this one.

## Open questions — answer with code, before implementing the dependent part

1. **`edited_content` after a relay re-fetch.** A pre-dispatch edit lives in
   queue memory (`queue.rs:55`), not in the signed event. Does re-fetching by id
   return the original body or the edited one? If the original, retry re-runs
   the wrong text and phase 2 needs the edit re-resolved.
2. **Batch granularity.** One notice can carry several `e` tags. Retry rebuilds
   the whole batch (leaning this way — the batch is the unit that failed) or one
   event at a time? Settle it against the queue code, not by preference.

## Acceptance criteria

1. Failure notice is identifiable and its failed event ids readable without
   inspecting the message copy.
2. Retry on an exhausted notice re-runs the original request; no second user
   message appears in the channel.
3. Retry when the relay no longer holds the events, and retry while a turn is
   already running, each report something distinct — not a silent no-op.
4. A retrying turn shows its attempt count on the same chrome Stop attaches to.
5. A batch dropped through the panic path posts a notice.
6. Full gate green, including `cd desktop && pnpm build` (the only step that
   runs `tsc`) and `cargo test -p buzz-acp --lib` under `env -u BUZZ_ACP_LAZY_POOL`.
