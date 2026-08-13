# Spike 0038 — Manager-chain under reorg race (#198)

- **Status:** PASS (fixture)
- **Date:** 2026-08-13
- **Issue:** [#198](https://github.com/Nuncio-hq/crew/issues/198)

## Question

If a new roster event arrives while a turn is in flight, which tree does
`handoff_should_create_work` use?

## Decision affected

D-060: LWW by `created_at` then event id; in-flight turn keeps its clone.

## Hypothesis

NIP-33 replaceable LWW is already the storage rule. The ACP cache should
match: replace only when the inbound 30680 is newer. A turn that already
cloned the roster for ORG-CHECK / budget should not flip mid-turn.

## Scope

- `OrgRosterCache` (`RwLock<Option<CachedOrgRoster>>`)
- inbound 30680 `cache_inbound_roster`

## Exclusions

Multi-relay clock skew beyond `created_at` + id.

## Pass criteria

Older event does not replace a newer cache. Equal `created_at` uses
lexicographically greater event id. In-flight clone is stable.

## Fail criteria

Cache flap that lets a peer handoff become work mid-turn.

## Environment

Unit tests on cache compare (`created_at`, then id).

## Method

`cache_inbound_roster` compares coordinates before replacing.

## Results

Newer wins. Turn-start reads a clone. Reorg takes effect on the next
turn or the next targeting handoff fetch.

## Edge cases observed

Empty cache fails the kickoff gate closed (conversation only) until a
roster is fetched.

## Limitations

Fixture only; no live two-publisher race.

## Verdict

PASS (fixture).

## Follow-up test contract

Keep cache compare tests next to `CachedOrgRoster`.

## Cleanup

None.
