---
phase: 4
title: "Upstream Sync CI and smoke tests"
status: pending
priority: P1
effort: "2-4h"
dependencies: [3]
---

# Phase 4: Upstream Sync CI and smoke tests

## Overview

Push the sync branch, run the manual **NuncioCrew Upstream Sync** workflow on
that ref, wait for **NuncioCrew Gate** on the PR (Phase 5 opens PR — may
overlap), and execute a focused smoke matrix for 0.5.8–0.5.10 behavior plus
Crew fork seams.

## Requirements

- Functional: Upstream Sync job green on exact sync HEAD; Crew-critical smokes pass.
- Non-functional: Smoke is evidence-based (commands + pass/fail), not vibes.

## Test matrix

### A. CI / Actions (required)

| Check | How | Pass means |
|---|---|---|
| NuncioCrew Upstream Sync | `gh workflow run nuncio-crew-upstream-sync.yml --ref sync/upstream-2026-08-12` | Root fmt/clippy/unit + heavier Tauri checks green; run SHA == branch HEAD |
| NuncioCrew Gate | Opens with PR | CI Policy + Desktop Fast / Desktop Rust / Package / etc. as path-gated |
| Desktop Smoke E2E | Advisory on Gate | Failures triaged; do not treat as sole merge blocker (D-032) but fix Crew regressions |

Confirm Upstream Sync run head SHA:

```bash
gh run list --workflow=nuncio-crew-upstream-sync.yml --branch sync/upstream-2026-08-12 --limit 5
```

### B. Automated local / PR e2e (strongly recommended)

```bash
cd desktop
pnpm test:e2e:smoke
```

Prioritize / re-run if flaky:

- Evidence: `evidence-cards.spec.ts`, `evidence-reactions.spec.ts`
- Messaging / send-to-channel: `messaging.spec.ts`
- Agents: `agents.spec.ts`, onboarding agent defaults
- Search scope: `search-scope-screenshots.spec.ts` (if registered)
- Channels invites / mute: `channels.spec.ts`

Always use `pnpm build:e2e` path (never plain `pnpm build` for mock bridge).

### C. Manual product smoke (NuncioCrew Local or dev)

| # | Scenario | Expect |
|---|---|---|
| 1 | Cold start → channel list | Faster / no obvious hang vs 0.5.7; no blank forever |
| 2 | Switch channels + return focus to app | No thrash refetch; timeline stays fresh |
| 3 | Scroll long channel history | Retention bound — app stays responsive |
| 4 | Open Settings | Glass / cohesive settings render; Crew version pin readable |
| 5 | Theme light/dark + open Communities | Theme preserved (#5266) |
| 6 | Thread → Send to channel | Message lands in target channel; thread link/affordance OK |
| 7 | Evidence card on agent completion | Card renders; accept/reject if applicable still works |
| 8 | Add / edit agent (unified flow) | Persona catalog + definition dialog open without dead ends |
| 9 | Agent turn (Hermes or stock) | Completes; no ACP prompt regression; permissions still approvable |
| 10 | Search scoped results | Scoping UI matches upstream intent; Crew routes still reachable |
| 11 | Private channel invite (if test community available) | Invite restores (#5493) |
| 12 | Link preview YouTube + Buzz entity link | Cards render |

Record results in `plans/260812-1158-upstream-sync-v0510/reports/` (create if needed): one markdown with command, SHA, pass/fail.

### D. Regression watchouts from this release train

- **Do not land on 0.5.9 mentally** — verify 0.5.10 perf fixes are present (timeline retention + get_channels work).
- NIP-AM archive: no crash on agent turns; archive sync still starts.
- Relay: if local relay used, reaction on project/repo events must not panic ingest.

## Implementation Steps

1. Push sync branch (after Phase 3):

```bash
git push -u origin sync/upstream-2026-08-12
```

2. Dispatch Upstream Sync on **the branch**, not default main:

```bash
gh workflow run nuncio-crew-upstream-sync.yml --ref sync/upstream-2026-08-12
```

3. Disable any newly imported out-of-scope workflows (`gh workflow list --all`) per UPSTREAM-SYNC.md.

4. Run smoke matrix B + C; file failures as sync-branch commits (fix, don't paper over).

5. Capture fork delta for PR body:

```bash
git diff --stat desktop-v0.5.10...HEAD
git diff --name-status desktop-v0.5.10...HEAD | head -80
```

## Success Criteria

- [ ] Upstream Sync workflow green on sync HEAD
- [ ] Evidence + messaging + agents smoke not regressing vs pre-sync baseline
- [ ] Manual checklist 1–10 executed (11–12 if environment allows)
- [ ] Report note lists any accepted advisory E2E flakes with links
- [ ] No unresolved merge markers / TODO(sync) left in tree

## Risk Assessment

| Risk | Mitigation |
|---|---|
| Advisory E2E flakes look like product bugs | Re-run once; compare to main; only fix if new on sync |
| Perf subjective | Prefer before/after feel + absence of known 0.5.9 hang patterns |
| Hermes live cert optional | If Hermes env missing, note INCONCLUSIVE for #9 stock-only |

## Next Steps

Phase 5 — update pin/docs and open sync PR to `Nuncio-hq/crew`.
