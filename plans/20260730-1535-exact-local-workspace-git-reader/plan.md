# Exact local workspace Git reader

- **Status:** Complete
- **Date:** 2026-07-30

## Goal

Make a folder-first Project with `.git` immediately readable through Buzz's
existing Local repository view, using the exact selected path and no Rust
change.

## Scope

1. [x] [RED contracts and resolver](phase-01-exact-path-resolver.md)
2. [x] Wire exact read-only snapshots into detail and overview.
3. [x] Show truthful Local checking/ready/unavailable states.
4. [x] Verify and rebuild NuncioCrew. The app-level data smoke needs an
   existing relay Project; Computer Use found none and did not create one.

## Locked boundaries

- No clone, fetch, pull, push, branch mutation, terminal, or session cwd.
- No Rust or Cargo changes.
- Never fall back to a same-named configured checkout.
- Symlink/unreadable/non-Git paths fail closed.
- Preserve Buzz's existing UI and relay Project identity.

## Dependency

- [Spike 0006](../../docs/crew/spikes/0006-reuse-existing-git-reader-for-exact-local-workspace.md)
