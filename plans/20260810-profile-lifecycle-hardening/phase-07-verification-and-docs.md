---
phase: 07
title: Verification, fault injection, and docs
status: planned
priority: P1
effort: M (2 d)
dependencies: ["03", "06"]
---

# Phase 07 — Verification, fault injection, and docs

## Outcome

The issue's Verification section is executed against real state, the
evidence is on the PR, and the durable docs tell the truth about what
shipped.

DoD coverage: #5 (documentation half), #6 (whole).

## Scope note

This phase owns the **end-to-end live verification** and the
**narrative docs consolidation**. It does not own per-PR STATE.md
updates — those are each shipping phase's obligation (#117 anti-drift,
plan DD-8). If phase 07 discovers STATE.md is stale, that is a defect in
the earlier phase, recorded as such.

## Work

### 1. Fault-injection probe (issue Verification bullet 1)

On a scratch profile, for each state, record the observed card state,
the spawn behavior, and the attention/Needs-You item:

| Injection | Expected |
| --------- | -------- |
| Corrupt `config.yaml` | card `broken-config`; spawn preflight fails fast with actionable copy |
| Remove `hermes` from the PATH the app sees | card `binary-missing` |
| Delete the profile directory | card `missing` |
| Healthy profile | card `ready`, with `auth-unknown` noted honestly (informational, not an error) |

Also verify each state **clears** after repair without an app restart.

### 2. Archive round-trip live (issue Verification bullet 2)

Offboard a scratch agent with archive → inspect the archive: caches
excluded, manifest present and correct → restore → re-bind → the agent
answers a mention **with profile memory intact**. Memory intactness is
the real assertion; a byte-count check is not sufficient evidence.

### 3. Guard live (issue Verification bullet 3)

Attempt archive while the agent is running → blocked with the reason.
Stop the agent → succeeds. Note in the record that stop is the existing
immediate `Child::kill()` path, not a graceful SIGTERM sequence (plan
DD-4) — so reviewers do not read the guard copy as promising a graceful
drain.

### 4. Playwright + screenshot evidence (issue Verification bullet 4)

Specs from phases 05 and 06 registered in
`desktop/playwright.config.ts` (`smoke` project `testMatch`). Build with
`pnpm build:e2e`; prefer `pnpm test:e2e:smoke` over a manual build plus
`playwright test`. Distinct-state gate:
`shasum -a 256 test-results/<dir>/*.png` — every hash unique. Post via
`scripts/post-screenshots.sh <pr> <dir> [body.md]` with `{{filename}}`
placeholders; delete superseded screenshot comments so reviewers see
only the current set.

### 5. Docs

- **`docs/crew/HERMES.md`**:
  - § Offboarding (`:137`) — keep vs **archive**; restore + re-bind;
    permanent delete only on archives behind type-name confirmation;
    running-agent guard. Keep the CLI fallback and the spike-0011 `-y`
    warning (`:154`) intact.
  - § Failure classes (`:157`) — rows for `broken-config` and
    `binary-missing` alongside the existing orphan rows.
  - § Known gaps (`:197`) — `auth-unknown` is now *surfaced honestly*,
    still blocked on the Hermes-side probe; keep the spike-0010 link
    (DoD #5's durable citation).
  - Archive location, format, and exclusion list, citing spike 0015.
- **`docs/crew/STATE.md`**: consolidate the shipped state for #119.
  While here, reconcile two known staleness points found during
  planning — Slice 3 still described as an upstream tier-1 PR to
  `block/buzz` (superseded by D-020), and Slice 4 described as a future
  gate though Phase 03/04 shipped. Fixing them is in scope for this
  phase; do not let the file keep contradicting D-020.
- **`docs/crew/DECISIONS.md`**: D-028 (next free — D-027 is the highest
  existing entry) — archive-on-offboard semantics
  (archive replaces destructive delete; permanent delete only on
  archives; guard refuses while running; archive location + format).
  Brief, per the issue's recommendation (OQ-5).

## Files

- **Modify:** `docs/crew/HERMES.md`, `docs/crew/STATE.md`,
  `docs/crew/DECISIONS.md`
- **Create:** `docs/crew/verification/0007-profile-lifecycle-hardening.md`
  (next free — 0006 is the highest existing) — the fault-injection and
  round-trip transcript
- **Modify:** `desktop/playwright.config.ts` if new specs need
  registration
- **Must not touch:** product code, except spec registration and any
  defect fix this phase's own verification exposes

## Validation

- All six issue DoD checkboxes demonstrably satisfied, each with named
  evidence (test name, transcript section, or screenshot).
- Every fault-injection row observed and recorded — not inferred from
  unit tests.
- Docs claims verified against source after editing
  (`documentation-management` rule: read before updating, verify links
  and claims after).
- `just ci` green; desktop suite green.
- If any earlier phase's STATE.md update was missed, that is recorded as
  a defect rather than quietly patched here.

## Risk and rollback

- **Risk:** live verification surfaces a defect late. That is the
  phase's purpose — fix in the owning phase's area and re-verify; do not
  weaken the check to pass.
- **Risk:** `auth-unknown` copy reads as a Crew fault rather than a
  known upstream limit. Mitigation: docs and UI both point at the
  spike-0010 ask.
- **Rollback:** docs-only revert; verification record retained (a
  stateful record, not evergreen authority).

## PR

Branch `agents/profile-lifecycle-docs`. Target `Nuncio-hq/crew`
(D-020). `git commit -s`. This is the PR that closes #119.
