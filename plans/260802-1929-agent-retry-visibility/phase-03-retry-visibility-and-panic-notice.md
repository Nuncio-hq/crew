# Phase 3 — Retry visibility + the panic hole

**Depends on:** [phase 1](phase-01-machine-readable-failure-notice.md) for the
notice tags. Independent of phase 2 otherwise.

## Context

Two silences, both the same class PR #24 removed — state the harness holds that
never reaches the person.

**Retrying is invisible.** `requeue` is called from two branches in
`handle_prompt_result` (`crates/buzz-acp/src/lib.rs:3599` and `lib.rs:3624`) and
emits nothing. With backoff up to `MAX_RETRIES` = 10 (`queue.rs:30`), a channel
can spend minutes retrying while the person sees an agent that has simply gone
quiet.

**The panic path swallows the batch.** `lib.rs:3913`:

```rust
let _ = queue.requeue(batch);
```

When that returns the dead-lettered batch, it is dropped with no notice. The
comment reasons that a panic has no outcome to report — true of the *outcome*,
but the person's message disappearing is itself the thing worth reporting, and
it is exactly the silent-drop failure this line of work exists to remove.

## Requirements

1. The requeue branches emit an observer frame carrying the attempt number and
   the maximum, so the desktop can show which attempt is running.
2. The desktop renders it on the same activity chrome Stop attaches to — not a
   new surface. Text uses rem tokens; `ChannelPane` stays under the ratchet.
3. The panic path posts a failure notice with reason class `panic`, tagged the
   same way as phase 1's other three call sites.

## Risk

An attempt counter on the rail competes for space with the elapsed/stuck/silence
states #22 introduced, and with Stop. Fit it into that chrome rather than
stacking another indicator beside it.

## Validation

- Unit: both requeue branches emit an attempt frame; the counter matches the
  queue's own count rather than a separately tracked number.
- Unit: a batch dead-lettered through the panic path posts a notice.
- Desktop unit: attempt state renders on the existing chrome and clears when the
  turn starts or the batch dead-letters.
- Full gate including `cd desktop && pnpm build`.
