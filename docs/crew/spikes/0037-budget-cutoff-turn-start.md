# Spike 0037 — Budget cut-off is turn-start, not mid-turn (#198)

- **Status:** PASS
- **Date:** 2026-08-13
- **Issue:** [#198](https://github.com/Nuncio-hq/crew/issues/198)

## Question

Can we abort an in-flight LLM turn when a token/open-work ceiling is
hit, or is turn-start the contract?

## Decision affected

D-060: turn-start only. Mid-turn abort is not the product contract.

## Hypothesis

Existing turn/token metrics are available at turn start. Aborting an
in-flight ACP session mid-stream is racy (partial tool calls, worktree
leases) and would surprise the founder. Next-turn enforcement plus
stop-and-report is enough.

## Scope

- `OrgBudgetTracker` + `SelfInitiatedTurnGuard` in `buzz-acp`
- Turn-start check in `pool.rs` **before** workspace bind

## Exclusions

Streaming token counters mid-completion.

## Pass criteria

Self-initiated turn is refused at start when tokens/day or open-work cap
is exhausted, and a `crew-budget stop` message is published. Founder
handoffs / assigned work are never blocked. Guard does not hold a
worktree lease.

## Fail criteria

Silent overage, or blocking founder-assigned work, or lease held after
ChannelWorkspace drop.

## Environment

Unit tests in `crates/buzz-acp/src/org_roster.rs`.

## Method

`evaluate_budget` at turn start. Assigned = inbound already passed the
handoff gate. Heartbeat without that flag is self-initiated.

## Results

Turn-start is the contract. First session after a cold start may
fail-open for one turn if the roster is not cached yet.

## Edge cases observed

Child ⊆ parent is checked at appointment (ingest) and again at turn
start against the cached roster.

## Limitations

No live token meter from the model provider; harness uses its existing
turn metrics.

## Verdict

PASS.

## Follow-up test contract

Keep budget unit tests; `resolve_and_bind_fails_closed_when_an_exclusive_eviction_lease_is_held` must stay green.

## Cleanup

None.
