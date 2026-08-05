# Phase 5 — Optional quota/LRU policy gate

## Status

**Deferred / follow-up — not implemented.** Do not treat Phases 1–4 completion as
permission to ship Phase 5. Open a separate follow-up issue only after production
evidence from the manual reclaim flow. See [README.md](README.md) delivery table.

## Goal

Keep disk usage bounded without ever deleting source work, branches, or human
checkouts automatically.

## Entry criteria

Do not implement this phase until all are true:

- lease refusal and cached-session reattach have shipped and been exercised;
- durable `lastUsedAt` has survived app/agent restarts;
- cache category deletion has no known false positives;
- manual preview/result telemetry is sufficient to choose sane defaults;
- the manager explicitly opts in per repository or Project.

## Policy order

When a configured local quota is exceeded:

1. prune broken Git worktree registrations;
2. clear allowlisted generated caches from clean or dirty, unleased worktrees in
   least-recently-used order;
3. if still over quota, evict only verified same-project worktrees that are clean,
   unleased, have no open PR, and exceed the configured idle age;
4. stop and notify when only active, dirty, protected, legacy/conflict, external,
   or open-PR worktrees remain.

Never automatically:

- pass `--force`;
- delete a branch or remote branch;
- close a pull request;
- remove an external/human worktree;
- delete unknown ignored/local state without the policy's explicit protected-state
  rule.

## Product controls

- off by default;
- local disk quota and minimum idle age;
- preview of the next candidates;
- last-run summary with path-safe labels, bytes reclaimed, and refusal reasons;
- `Run now` uses the same backend authorization as scheduled cleanup;
- one-click disable.

## Tests

- deterministic LRU ordering from durable `lastUsedAt`;
- cache-first ordering;
- quota stop once enough bytes are reclaimed;
- active/dirty/open-PR/other-channel/legacy/external entries are skipped;
- races become typed refusals, not forced cleanup;
- branches remain after every automatic eviction;
- policy off means no background filesystem mutation.

## Exit gate

A production-like fixture can exceed quota, reclaim only allowed cache/checkouts,
retain every branch, and reattach an evicted thread on its next turn. Until that
gate is demonstrated, manual reclaim remains the shipped behavior.
