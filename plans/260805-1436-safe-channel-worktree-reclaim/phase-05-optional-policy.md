# Phase 5 — Optional quota/LRU policy gate

## Status

**Superseded / completed by [#174](https://github.com/Nuncio-hq/crew/issues/174).**

The original Phase 5 draft described background/opt-in quota + LRU auto-GC.
#59's P3 precondition (manual reclaim proven safe) was met by PR #72; #174
completes P3 with a different product posture:

- **Suggest-and-confirm** bulk flow (no background auto-GC)
- **Observed-time idle** via an app-scoped alive-interval ledger (not
  wall-clock idle that storms after absence)
- Lean (cache sweep) / Hibernate (evict when clean + merged/pushed) tiers
  over the existing #72 primitives only
- PR merged via worktree-registry PR state (squash-safe), never git ancestry

See `docs/crew/DECISIONS.md` D-051 and issue #174. A future opt-in scheduled
cache-only sweep remains a separate founder decision and is still a non-goal
of #174.
