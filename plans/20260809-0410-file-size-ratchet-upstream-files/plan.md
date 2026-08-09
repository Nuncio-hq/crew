# Plan — #111 File-size ratchet vs upstream-owned files

Spec: [#111](https://github.com/Nuncio-hq/crew/issues/111)
Status: not started. **Deliberately deferred behind #109** — see below.

## Outcome

The next upstream sync does not fail `Desktop Fast` on a file whose growth Crew
does not control, and the rule that decides this is written down rather than
re-litigated mid-merge.

## Current state (verified at `origin/main` = `e41a1a6a4`)

```
999  desktop/src/features/messages/ui/MessageThreadPanel.tsx   ← 1 line of headroom
986  desktop/src/features/channels/ui/ChannelPane.tsx
972  desktop/src/features/messages/ui/MessageRow.tsx
```

## Why it is deferred, not forgotten

The fix for this issue is an **extraction** — and extraction in exactly this file
is the change class that produced the code-alive-but-unwired regressions during
the v0.5.5 and v0.5.7 syncs. Both were caught by e2e or human review, not by unit
tests or typecheck:

- the v0.5.7 merge silently dropped Crew's existing `message-thread-panel-head.tsx`
  extraction, orphaning a live 103-line Crew file;
- the recovery briefly produced two components with the same name and different
  prop APIs.

e2e shard 4 — the net for that class — is currently dead (#109). Doing UI surgery
that depends on the net before restoring the net is the wrong order. **Restore
#109 first.**

## The real problem is a rule conflict, not a line count

D-022 says: extract Crew's additions, and **do not restructure upstream's own code
just to pass the guard.** For this file the excess is upstream's — it was 987
lines at `desktop-v0.5.7` and 1043 after the merge. Both halves of D-022 cannot be
satisfied at once. That conflict, not the single line of headroom, is what needs a
decision.

## Options

1. **Pre-emptive extraction of Crew's remaining delta** in this file — done now,
   with attention, instead of mid-sync under time pressure.
2. **Treat the ratchet as a true ratchet for upstream-heavy files** — "must not
   grow" against a recorded baseline, the way `discovery.rs` (1494/1495) is
   already handled — instead of a hard 1000. Requires a change to
   `check-file-sizes-core.mjs` plus a `DECISIONS.md` entry.
3. Do nothing; absorb the breakage during the next sync.

Recommendation: **(2)**. A fixed limit on a file whose growth Crew does not
control turns the guard into a tax on syncing, and the guard's purpose is to stop
*Crew* from making files worse. (1) is worth doing as well if the file keeps
growing, but it does not solve the recurrence.

## Constraints

- **Never** raise `MAX_LINES` in `check-file-sizes.mjs` as a blanket escape hatch.
- **Never** add a per-file override to slip under the guard.
- A source-scanning contract test goes blind when code moves to another file. After
  any extract: repoint the test, keep every `>= 0` guard, and re-read the invariant
  by hand — the guard is off in the interval.
- Option (2) edits CI policy → separate PR, and outside my standing merge authority.
  Needs Oscar's explicit approval.

## Acceptance criteria

- Chosen option recorded in `docs/crew/DECISIONS.md` with its reasoning.
- If (2): `check-file-sizes-core.mjs` change is covered by a contract test that
  fails when a baseline is raised rather than held.
- If (1): the mutation check applies — the extracted code must be proven still
  *called*, not merely still present.

## Dependencies

Blocked on **#109** (e2e shard 4) for option (1). Option (2) is independent and
could land first, since it is a CI-policy change with no runtime behaviour.
