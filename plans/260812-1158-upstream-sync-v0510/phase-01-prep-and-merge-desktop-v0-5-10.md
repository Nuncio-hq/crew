---
phase: 1
title: "Prep and merge desktop-v0.5.10"
status: pending
priority: P1
effort: "1-2h"
dependencies: []
---

# Phase 1: Prep and merge desktop-v0.5.10

## Overview

Prepare a clean Crew `main`, cut `sync/upstream-2026-08-12`, and merge the
**release tag** `desktop-v0.5.10` (not `upstream/main`). Leave conflicts for
Phase 2 — do not force-resolve here beyond listing them.

## Requirements

- Functional: sync branch contains an attempted merge of `desktop-v0.5.10` onto current `origin/main`.
- Non-functional: worktree uses Hermit; no push to `block/buzz`.

## Related Code Files

- Read: `docs/crew/UPSTREAM-SYNC.md`, `docs/crew/upstream-buzz.json`, `docs/crew/STATE.md`
- Modify later: conflicted files (Phase 2), pin JSON (Phase 5)

## Implementation Steps

1. Activate toolchain: `. ./bin/activate-hermit`
2. Confirm clean tree and remotes:

```bash
git status --short --branch
git fetch --prune origin
git fetch --prune upstream --tags
git rev-parse desktop-v0.5.10   # expect 1fb49103002e898607a7f6fd554cb51e94d92e08
```

3. Sync local main:

```bash
git switch main
git pull --ff-only origin main
```

If `main` is dirty, **stop**.

4. Cut sync branch and merge the tag:

```bash
git switch -c sync/upstream-2026-08-12
git merge --no-edit desktop-v0.5.10
```

5. Capture conflict inventory (do not resolve yet if interrupting):

```bash
git diff --name-only --diff-filter=U | tee /tmp/crew-sync-0510-conflicts.txt
wc -l /tmp/crew-sync-0510-conflicts.txt
```

Expected order of magnitude: ~60–70 paths (merge-tree predicted 67).

6. Confirm ancestry intent:

```bash
# After conflicts resolved + commit (Phase 2):
git merge-base --is-ancestor desktop-v0.5.10 HEAD
```

## Success Criteria

- [ ] Branch `sync/upstream-2026-08-12` exists from up-to-date `origin/main`
- [ ] Merge of `desktop-v0.5.10` started (or completed with conflicts listed)
- [ ] Conflict list saved / pasted into Phase 2 notes if count differs materially from 67
- [ ] No commit yet that drops Crew deltas without review

## Risk Assessment

| Risk | Mitigation |
|---|---|
| Accidentally merge `upstream/main` | Script/commands use tag only |
| Working on dirty / wrong worktree branch | Status check before switch |
| Tag missing locally | `git fetch --prune upstream --tags` |

## Next Steps

Phase 2 — resolve conflicts with take-upstream + re-apply Crew delta rule.
