---
title: "Upstream sync Buzz Desktop v0.5.7 to v0.5.10"
description: "Merge Buzz desktop-v0.5.10 into Crew, preserve Crew fork deltas, run Upstream Sync + smoke, then pin docs and open PR."
status: pending
priority: P1
branch: "sync/upstream-2026-08-12"
tags: [upstream-sync, buzz, desktop]
blockedBy: []
blocks: []
created: "2026-08-12T02:28:43.240Z"
createdBy: "ck:plan"
source: skill
---

# Upstream sync Buzz Desktop v0.5.7 → v0.5.10

Status: **planned, not started**. Created 2026-08-12.

## Sync target (release tag, not upstream/main)

| | Value |
|---|---|
| Current pin | `0.5.7` / `desktop-v0.5.7` / `f167818d25dd9f03115ab907a16f07daee2ece5c` |
| New pin | `0.5.10` / `desktop-v0.5.10` / `1fb49103002e898607a7f6fd554cb51e94d92e08` |
| Published | [Buzz Desktop v0.5.10](https://github.com/block/buzz/releases/tag/desktop-v0.5.10) (2026-08-12) |
| Gap | 3 releases · ~55 commits · ~384 files |
| Predicted conflicts | ~67 files (`git merge-tree origin/main…desktop-v0.5.10`) |

**Rule:** merge the release tag `desktop-v0.5.10`, never `upstream/main`. Tag is immutable; main drifts.

**Why not stop at 0.5.9:** `0.5.10` removes 0.5.9+ desktop perf regressions and speeds up `get_channels`. Pinning 0.5.9 risks a worse UX than today's 0.5.7 pin.

## Manager outcome

- Crew desktop rides Buzz `0.5.10` behavior (perf, glass/settings, agents, send-to-channel, search).
- Crew evidence cards, ACP office prompt assertion, and Hermes/agent surfaces still work.
- `docs/crew/upstream-buzz.json` + Settings pin show `v0.5.10`.
- Sync lands via reviewed PR to `Nuncio-hq/crew` (never `block/buzz`).

## What upstream shipped (Crew-relevant)

### Must take

| Area | Why |
|---|---|
| Desktop perf hotfix stack (0.5.10) | Timeline retention, read-state coalesce, focus refetch suppress, `get_channels` |
| Glass appearance + cohesive settings | Large theme/settings rewrite — expect visual QA |
| Unify add-agent flows + NIP-AM agent-usage archive | Agent UX + Tauri archive surface |
| Send to channel from threads | Touches `MessageRow.tsx` (Crew evidence prop seam) |
| ACP session-context trim + permission revert | Agent harness behavior |
| Relay ingest panic on project reactions | Stability if Crew runs near-upstream relay |

### Nice / neutral

Search scoping, link-preview fixes, private-channel invites, onboarding polish, localStorage bounds/sweep, CLI `--visibility`, mesh cleanup.

## Conflict clusters (predicted)

| Cluster | Count (approx) | Default resolve |
|---|---|---|
| Root / release / lockfiles | 8 | Prefer upstream; re-apply Crew Justfile/Cargo workspace deltas |
| `crates/buzz-acp` | 5 | Upstream + keep Crew mods / prompt assertion / office rules |
| `crates/buzz-cli` | 1 | Upstream + keep evidence flag wiring |
| Desktop Tauri | 9 | Upstream + keep Crew identity / managed-agent hooks |
| Desktop agents / channels / messages | ~25 | Upstream + re-apply evidence / Crew UI seams |
| Settings / sidebar / shared API | ~12 | Upstream; visual check glass theme |
| Playwright / e2e / lock | 5 | Upstream + keep Crew evidence testMatch |

High-care Crew-touched files (from `UPSTREAM-SYNC.md`):

- `crates/buzz-acp/src/lib.rs` — **conflict likely**
- `crates/buzz-cli/src/lib.rs` — **conflict likely**
- `desktop/playwright.config.ts` — **conflict likely**
- `desktop/src/features/messages/ui/MessageRow.tsx` — **conflict likely** (Send-to-channel)
- `MessageRowDefaultBody.tsx`, `AgentReceiptCard.tsx`, `evidence.rs`, `base_prompt.md` — no upstream change in 0.5.7→0.5.10 range, but verify they still compile/wire after merge

## Phases

| Phase | Name | Status |
|-------|------|--------|
| 1 | [Prep and merge desktop-v0.5.10](./phase-01-prep-and-merge-desktop-v0-5-10.md) | Pending |
| 2 | [Resolve Crew fork conflicts](./phase-02-resolve-crew-fork-conflicts.md) | Pending |
| 3 | [Local quality gates](./phase-03-local-quality-gates.md) | Pending |
| 4 | [Upstream Sync CI and smoke tests](./phase-04-upstream-sync-ci-and-smoke-tests.md) | Pending |
| 5 | [Pin docs and open PR](./phase-05-pin-docs-and-open-pr.md) | Pending |

## Acceptance criteria

- [ ] `git merge-base --is-ancestor desktop-v0.5.10 HEAD`
- [ ] `docs/crew/upstream-buzz.json` = `0.5.10` / `desktop-v0.5.10` / `1fb49103002e898607a7f6fd554cb51e94d92e08`
- [ ] `NuncioCrew Upstream Sync` green on sync branch HEAD
- [ ] `NuncioCrew Gate` green on sync PR
- [ ] Evidence card smoke + agent mention/dispatch smoke pass (local or advisory E2E)
- [ ] Settings reports `v0.5.10 · Local` on NuncioCrew Local build (or docs pin updated if Local not rebuilt in this PR)
- [ ] Fork delta reviewed: `git diff --stat upstream/desktop-v0.5.10...HEAD` shows only intentional Crew files

## Risks

| Risk | Mitigation |
|---|---|
| ~67 conflicts drop Crew evidence/ACP seams | Phase 2 checklist + evidence e2e specs |
| Glass/settings rewrite clashes with Crew SettingsView | Prefer upstream panels; re-apply Crew-only rows; visual smoke |
| Worktree `desktop-tauri-fmt` blocks commit | Run fmt from main checkout per AGENTS.md gotcha |
| File-size baselines (D-022) trip on upstream growth | Update recorded baselines only for upstream line growth |
| Syncing `upstream/main` by mistake | Always merge tag `desktop-v0.5.10` |

## Out of scope

- Cherry-picking individual PRs instead of the release tag
- Enabling inherited Buzz multi-platform CI
- Mobile / Windows / Linux product claims
- Publishing a Crew release in the same PR (optional follow-up after pin)

## Related

- Runbook: `docs/crew/UPSTREAM-SYNC.md`
- Prior sync plan pattern: `plans/260805-1201-upstream-sync-v055/`
- Identity: `docs/crew/IDENTITY.md` (PRs → `Nuncio-hq/crew` only)

## Decisions (locked 2026-08-12)

1. **No Local rebuild / signing** in this sync — pin docs + land on `main` only.
2. **No special Settings survival** beyond normal pin/docs — take upstream glass/settings as-is.
3. **Execute now:** fix until CI green, then merge to `main`.
