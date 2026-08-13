# Spike 0033 — `describe-ui` ref stability across two snapshots (#197)

- **Status:** INCONCLUSIVE (live idb/baguette) / PASS (in-repo)
- **Date:** 2026-08-13
- **Issue:** [#197](https://github.com/Nuncio-hq/crew/issues/197)

## Question

Do two consecutive AX snapshots of the same simulator screen yield
identical `e1..eN` refs?

## Decision affected

D-059 — one snapshot format for browser and sim; refs are digest-scoped.

## Hypothesis

`describe-ui` (baguette / idb) returns a stable accessibility tree for
an unchanged screen. The same tree-path + role/label assignment used
for the browser bridge produces identical refs.

## Scope

- Fake AX tree at the process boundary (no live `simctl`)
- Unified snapshot renderer in `agent_control`

## Exclusions

- Live idb-companion / baguette on this Linux VM
- WebDriverAgent / dylib injection (non-goal)

## Pass criteria

Two snapshots of the same fake AX tree → same refs + digest.

## Environment

- OS: Linux; `simctl` / idb / baguette **not** the live instrument here
- Bridge discovery ladder is the same as #196

## Results

Live describe-ui: INCONCLUSIVE (no simulator).
In-repo: `sim_snapshot_refs_match_browser_format` and stability tests
PASS against the fake bridge.

## Verdict

INCONCLUSIVE live / PASS in-repo. Production maps describe-ui JSON
through the same ref mint as the browser tree.

## Follow-up test contract

`sim_snapshot_stable_across_unchanged_tree`.

## Cleanup

None.
