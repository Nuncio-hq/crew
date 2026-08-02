# Phase 1 — Machine-readable failure notice

**Blocks:** phases 2 and 3. The tag shape settled here is what the desktop reads
and what the control frame carries, so it lands and gets reviewed on its own.

## Context

`spawn_failure_notice` (`crates/buzz-acp/src/lib.rs:3487`) hands the batch's
thread tags to `pool::post_failure_notice` (`crates/buzz-acp/src/pool.rs:4215`),
which builds the notice with:

```rust
buzz_sdk::build_message(channel_id, content, thread_ref.as_ref(), &[], false, &[])
```

`build_message` (`crates/buzz-sdk/src/builders.rs:220-238`) emits `h` plus thread
tags and nothing else. The notice therefore says *that* something failed, in
prose, and carries no machine-readable record of *what* failed.

Four call sites post one today, not three: hard-cap timeout with no recent
activity (immediate dead-letter, never touches the retry budget), hard-cap
exhaustion after requeue, auth, and generic exhaustion. Phase 3 adds a fifth for
the panic path. The first and second are different causes despite sharing the
"exceeded the maximum duration" wording, so the cause classes must distinguish
them.

## Requirements

1. The notice carries a marker tag identifying it as an agent failure notice,
   plus its reason class: `retry_exhausted`, `auth`, `panic`.
2. The notice carries one `e` tag per failed event, sourced from `batch.events`.
3. Kind stays **9**. Every client including mobile must keep rendering the
   prose; a new kind renders as nothing on surfaces that don't know it.
4. The prose stays human-readable on its own — the tags are additive, not a
   replacement for the copy.

## Files

- `crates/buzz-sdk/src/builders.rs` — new builder beside `build_message`.
  Do not add parameters to `build_message`; it already takes six and three call
  sites would gain arguments they never use.
- `crates/buzz-acp/src/pool.rs` — `post_failure_notice` uses the new builder.
- `crates/buzz-acp/src/lib.rs` — `spawn_failure_notice` passes the reason class
  and the failed event ids through.

## Risk

The `e` tags share a namespace with NIP-10 thread tags, which this notice
already emits via `thread_ref`. Reply markers and failed-event references must
stay distinguishable — a desktop reader that treats every `e` as a failed event
will try to retry the thread root. Settle this explicitly rather than relying on
tag order.

## Validation

- Unit: a notice built for a multi-event batch carries one reference per event
  and its reason class, and its thread tags stay intact and distinguishable.
- Unit: each of the three existing call sites tags its own reason class.
- `cargo test -p buzz-acp --lib` under `env -u BUZZ_ACP_LAZY_POOL`, plus
  `cargo test -p buzz-sdk`.
