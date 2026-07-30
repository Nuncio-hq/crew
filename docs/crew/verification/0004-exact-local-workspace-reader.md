# Verification 0004 — Exact local workspace reader

- **Date:** 2026-07-30
- **Result:** Automated and release gates passed; exact-data native smoke needs
  a relay Project fixture

## Behavior proved

A Project linked to a normal local Git worktree is read from the exact selected
folder. Crew derives the folder parent and basename, reuses Buzz's existing
native snapshot command, and rejects a result whose returned path differs from
the selected path.

The read-only snapshot supplies files, README, commits, contributors, and
language data. It never falls back to a same-named configured Buzz checkout or
remote clone. Clone, fetch, pull, push, Terminal, and configured-root commit
diff remain disabled for a linked workspace.

## TDD evidence

The focused contracts were RED before the resolver and wiring existed. The
final focused run passed:

```text
tests 19
pass 19
fail 0
```

Covered edge cases include Project identifier differing from folder basename,
Unicode/spaces, null and error results, mismatched native returned path, no
remote fallback, legacy Buzz clone-origin matching, truthful source labels, and
mutation-control suppression.

## Automated gates

```text
pnpm typecheck   passed
pnpm check       passed
pnpm test        3846 passed, 1 skipped, 0 failed
pnpm build:e2e   passed
```

The skipped test is the existing opt-in live relay boundary test. Biome emitted
two pre-existing informational template-literal suggestions outside this
feature. No Rust or Cargo production file changed.

## Native release gate

The release build completed and Computer Use opened the resulting
`NuncioCrew.app`. The Projects page loaded, but the configured relay returned
`No projects yet`. No Project was created implicitly because that would write
a signed event to the manager's real relay.

After an existing Project is linked, verify:

- Local becomes the selected source after a valid snapshot;
- real files, commits, contributors, README, and languages render;
- Fetch, sync, clone, and Terminal actions are absent;
- no relay or Git mutation is performed during the smoke.

Symlink-selected, missing, unreadable, and non-Git paths are intentionally
reported as `Local unavailable`.
