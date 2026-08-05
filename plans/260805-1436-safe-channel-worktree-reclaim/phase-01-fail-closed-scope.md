# Phase 1 — Fail-closed channel scope and truthful current UX

## Status

**Implemented / verified** on current `origin/main` (2026-08-05). See
[README.md](README.md) implementation verification. Phase 5 is not in scope here.

## Goal

Remove the two unsafe claims the current UI makes before introducing new
lifecycle machinery:

1. absence from the loaded timeline does not prove `other-channel`;
2. a clean Git status does not mean ignored/local files survive eviction.

This phase does not add automatic cleanup and does not broaden any destructive
command.

## Changes

### Bucket model

Update `desktop/src/features/channels/lib/worktreeBuckets.ts`:

- replace actionable `orphanReason: "other-channel"` with a non-actionable
  `channel-unknown` state until Phase 3 supplies durable routing-channel data;
- keep main/external entries read-only;
- keep a no-root legacy orphan visible, but do not present it as belonging to the
  current channel;
- centralize actionability in a pure helper such as `canReclaimWorktree(item)` so
  row and bulk selection cannot drift;
- check active presentation state before showing any reclaim affordance, while
  documenting that backend authorization arrives in Phase 2.

Update:

- `ChannelWorktreeRow.tsx` so unknown/other-channel rows have no checkbox or
  remove button;
- `ChannelWorktreesDrawerBuckets.tsx` labels and hints;
- `ChannelWorktreesDrawer.tsx` selected-state pruning so a refresh cannot leave
  a now-read-only path selected;
- `ChannelWorktreesRemoveDialog.tsx` copy to say every eviction keeps the branch
  and removes ignored/local files inside the checkout.

### Disk measurement

Remove the all-at-once prefetch in `ChannelWorktreesDrawer.tsx`. Load details on
row expansion, with a small concurrency limiter only if the drawer still needs a
total. Do not block registry rendering on `du`.

### Backend preview fact

Extend worktree detail/preview data with a boolean or typed state indicating that
ignored entries exist. Use `git status --porcelain --ignored --untracked-files=all`
only to classify presence; do not return file contents. Avoid returning secret
contents or logging ignored paths.

The backend remains the source of this fact. The UI must not walk the worktree.

## Tests

Frontend (verified):

- [x] a mapped root absent from the visible channel set is read-only `channel-unknown`;
- [x] unknown-channel and main/external rows cannot be selected individually or in
  bulk;
- [x] selected paths are pruned after registry refresh changes actionability;
- [x] confirm copy states branch retention and ignored-file deletion;
- [x] opening the drawer does not start details requests for every row.

Rust (verified):

- [x] ignored-file presence is detected without reading contents;
- [x] no ignored filename or content appears in command output/errors;
- [x] external/main/unregistered path guards remain unchanged.

## Exit gate

- [x] The drawer cannot remove a known root merely because its message is outside the
  current timeline window.
- [x] Current destructive controls tell the truth about branch retention and ignored
  files.
- [x] Existing cleanup guard and bucket test suites pass.

## Rollback

Revert the presentation changes. No metadata format or filesystem mutation is
introduced in this phase.
