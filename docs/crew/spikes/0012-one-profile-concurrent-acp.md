# Spike 0012 — One Hermes profile, concurrent ACP processes

- **Status:** PASS (bounded)
- **Date:** 2026-08-05
- **Feature:** [`../features/0001-hermes-first-class-runtime.md`](../features/0001-hermes-first-class-runtime.md) (S0-4)

## Question

Do two concurrent `hermes -p <profile> acp` processes (the shape
`BUZZ_ACP_AGENTS=2` produces) complete overlapping turns against one
profile store without lock errors, crashes, or cross-talk?

## Decision affected

C-11 (parallelism guard): whether Crew must cap Hermes agents at
`parallelism = 1` or may allow N > 1.

## Hypothesis

The profile `state.db` is SQLite; concurrent writers risk
`database is locked` unless WAL + busy-timeout handling exists.

## Scope

- Two processes, one overlapping trivial turn each, one profile
  (`crewspike`), probe archived at
  [`assets/0012-concurrent-profile-probe.py`](assets/0012-concurrent-profile-probe.py).

## Exclusions

- Sustained load, long turns, N > 2, tool-heavy turns, memory writes,
  session compression under contention — this is a smoke, not a soak.

## Pass criteria

Both replies correct and attributed to their own session; no
lock/traceback lines on either stderr; both exits 0.

## Fail criteria

Any SQLite lock error, crash, or cross-session reply bleed.

## Environment

Hermes v0.20.0, macOS 26.5.2 (local APFS home volume).

## Method

Spawn A and B concurrently; each runs initialize → session/new → prompt
("alpha" / "bravo"); assert reply matches its own word; grep both
stderr streams for `locked|sqlite|traceback`; check
`PRAGMA journal_mode` on the profile store afterwards.

## Results

- Elapsed 6.9s total; overlapping in time.
- A → sessionId `b2ae8db9…`, reply `alpha`, `end_turn`; B → sessionId
  `6d6982f5…`, reply `bravo`, `end_turn`. No cross-bleed.
- Zero lock/crash lines on either stderr; both exit 0.
- `PRAGMA journal_mode` on the profile `state.db` → `wal`; sessions
  table showed both sessions recorded (count 5 total for the profile
  after all spike runs).

## Edge cases observed

None in this bounded run.

## Limitations

- Two short, tool-less turns are far from `parallelism = 4` under
  sustained board load. WAL supports one writer at a time; heavier
  concurrent writes could still surface `SQLITE_BUSY` depending on
  Hermes' busy-timeout configuration (not inspected).
- Network filesystems (where WAL is unsupported) untested; irrelevant
  for the current one-machine product scope.

## Verdict

**PASS (bounded)** — no evidence that N=2 on one profile corrupts or
locks. Recommendation for C-11: default Hermes agents to
`parallelism = 1` in Crew UX (matching the one-agent-one-profile mental
model), allow raising it without a hard block, and revisit with a soak
test only if a real workload demands N > 1.

## Follow-up test contract

If a Crew surface ever sets `parallelism > 1` for a Hermes agent by
default, a soak-level contract test (long overlapping tool-using turns,
lock-error assertion) must exist first.

## Cleanup

Probe archived to `assets/`; `crewspike` profile deleted (spike 0011).
