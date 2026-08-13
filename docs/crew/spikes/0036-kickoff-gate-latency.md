# Spike 0036 — Kickoff gate must not slow ordinary wakes (#198)

- **Status:** PASS (fixture)
- **Date:** 2026-08-13
- **Issue:** [#198](https://github.com/Nuncio-hq/crew/issues/198)

## Question

Can `author_allowed` stay unchanged while the handoff gate fetches the
roster only when needed?

## Decision affected

D-060: gate after `author_allowed`; ordinary wakes do not fetch.

## Hypothesis

Sibling-wake / flat peer flow already works. Fetching 30680 on every
inbound event would add a relay round-trip to ordinary chat. Cache on
`PromptContext` and fetch only for (a) `crew-handoff` targeting this
agent, (b) fresh-session ORG-CHECK, (c) inbound 30680.

## Scope

- `crates/buzz-acp/src/org_roster.rs`
- inbound loop in `crates/buzz-acp/src/lib.rs`

## Exclusions

Production latency numbers on a loaded relay.

## Pass criteria

Ordinary inbound without a targeting handoff does not call roster fetch.
Handoff targeting self consults cache/fetch and returns conversation-only
when the author is off-chain (message remains on the relay).

## Fail criteria

Roster query on every wake, or silent drop of off-chain handoffs.

## Environment

Unit tests in `buzz-acp` `org_roster` + inbound continue-path.

## Method

Read the inbound loop: roster fetch is gated on handoff-targeting-self
and new-session ORG-CHECK. Budget uses the cache and fails open if empty.

## Results

Gate is after `author_allowed`. Off-chain handoff skips the turn but does
not delete the event. Ordinary wakes unchanged.

## Edge cases observed

First new-session turn may budget-fail-open because fetch happens after
the turn-start check; next turn uses cache (see 0037).

## Limitations

No live timing histogram.

## Verdict

PASS (fixture).

## Follow-up test contract

Keep `handoff_should_create_work` matrix tests.

## Cleanup

None.
