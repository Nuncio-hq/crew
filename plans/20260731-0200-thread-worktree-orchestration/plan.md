# Thread worktree orchestration

- **Status:** Complete
- **Date:** 2026-07-31

## Goal

Let one Project channel run several independent task threads concurrently,
with one deterministic Git worktree shared by every agent handoff in a thread.

## Phases

1. [x] Key ACP queue, affinity, session, control, and typing state by thread.
2. [x] Preserve the real NIP-29 channel for relay operations and UI activity.
3. [x] Encode trusted Project workspace context in the normal composer.
4. [x] Provision an idempotent, fail-closed worktree before `session/new`.
5. [x] Wake the first ordered agent while retaining later agent references.
6. [x] Complete desktop, Rust, concurrency, package, and CI verification.
7. [x] Record release-ready evidence and complete handoff.

## Invariants

- Normal channel messages and single-agent mentions keep existing behavior.
- No Project prompt button or modal; the composer remains the only entry point.
- A thread root is the idempotency key; prompt text never names a worktree.
- Project workspace metadata is trusted only when authored by the agent owner.
- Provisioning failure never falls back to the shared source checkout.
- Later agent handoffs resolve the same thread root and therefore the same cwd.
