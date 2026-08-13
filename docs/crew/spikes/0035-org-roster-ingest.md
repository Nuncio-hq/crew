# Spike 0035 — Org roster ingest rejects broken trees (#198)

- **Status:** PASS (fixture)
- **Date:** 2026-08-13
- **Issue:** [#198](https://github.com/Nuncio-hq/crew/issues/198)

## Question

Does pre-storage validation reject cycles, orphans, non-founder authors,
and unknown agents so a broken tree never exists on the relay?

## Decision affected

D-060: validate before storage, not as a post-insert side effect.

## Hypothesis

`validate_org_roster_event` plus owner check is enough. Git-announcement
post-insert logging is the wrong precedent for a tree that must not exist
in a bad state.

## Scope

- `crates/buzz-core/src/org_roster.rs` parser tests
- `crates/buzz-relay/src/handlers/org_roster.rs` ingest unit tests

## Exclusions

Live Postgres ingest against a running relay (optional follow-up).

## Pass criteria

Cycle, orphan, two-manager JSON, non-founder, unknown agent, founder-as-node
all return `OrgRosterError`. Empty founder-signed roster is accepted. A
second event replaces the tree (LWW).

## Fail criteria

Any of those fixtures stored or parsed as valid.

## Environment

- Fixture keys from `nostr::Keys::generate()`
- `cargo test -p buzz-core org_roster --offline`
- `cargo test -p buzz-relay org_roster --offline`

## Method

Unit tests in the Crew handler and core parser.

## Results

Fixture tests cover founder-empty OK, non-founder, wrong `d`, cycle, orphan,
unknown agent, and atomic reorg. Evidence: handler module tests.

## Edge cases observed

Two managers cannot appear as sibling keys on one agent; last JSON key
wins or the node is invalid. Documented in D-060.

## Limitations

No live relay in this spike; labeled fixture evidence.

## Verdict

PASS (fixture).

## Follow-up test contract

Keep the handler unit tests; add ingest integration when Postgres is up.

## Cleanup

None.
