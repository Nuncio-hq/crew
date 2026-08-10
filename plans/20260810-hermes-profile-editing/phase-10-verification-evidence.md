---
phase: 10
title: Verification & evidence
status: planned
priority: high
effort: S
dependencies: [09]
---

# Phase 10 — Verification & evidence

Issue #118 § Verification and DoD-5. Turns the work into evidence a reviewer can
check without re-running it.

## Automated gate

```bash
. ./bin/activate-hermit

just ci                       # fmt + clippy + desktop lint + unit tests + builds

# desktop crate is excluded from the root workspace — run it explicitly
cargo test --manifest-path desktop/src-tauri/Cargo.toml
cargo test -p buzz-acp system_prompt

cd desktop
pnpm check && pnpm check:px-text && pnpm check:file-sizes
pnpm test:e2e:smoke           # builds with the mock bridge; never `pnpm run build`
```

`just test` (integration, needs Postgres + Redis) is **not** required — no
relay, db, or auth crate is touched.

Every one of E-01…E-17 must be green, and C-05, C-06, C-07, C-10, C-12, C-13,
C-14, C-15 must still be green.

## Live probe (real profile, this machine)

The issue requires a live probe, not just mocks.

1. Read the model for a bound profile in Crew; cross-check with the Hermes CLI.
2. Change it from Crew; confirm the profile reflects it and Crew stores nothing.
3. Send `!rotate`, run a turn, confirm the new model is in effect (C-07).
4. Open `SOUL.md` in Crew, confirm it matches disk, edit, save, confirm the file
   changed and nothing else in the profile directory did.
5. Create an agent with an **empty** description; inspect the `session/new`
   payload and confirm no system-prompt field is present.
6. Attempt an invalid model id; confirm the classified error and that the agent
   still runs on its previous model (red-team R-4).
7. Confirm a non-Hermes runtime (Claude Code or Codex) renders exactly as before
   (C-15).

**Probe hygiene:** prefer a throwaway profile under a temporary `HERMES_HOME`
for destructive steps. Restore any value changed on a real profile. Never print
credential values or non-model config keys.

## Playwright evidence

Three states from the issue:

| Shot | State |
| ---- | ----- |
| 1 | Hermes agent in edit — editable model control + the shared-everywhere note |
| 2 | `SOUL.md` editor open, populated with real current content |
| 3 | Create/edit with an empty "Agent instructions" box, labelled optional, save enabled |

Rules that make this evidence trustworthy:

- Build with `pnpm build:e2e` / `pnpm test:e2e:smoke`. A plain `pnpm run build`
  strips the mock bridge and every spec fails looking like a product bug.
- Kill port 4173 before re-running after code changes (`reuseExistingServer`
  serves stale code).
- `waitForAnimations(page)` before every capture.
- Scope each shot with `locator.screenshot()` and **verify hashes are distinct**
  before posting — identical hashes mean two shots captured the same state:
  ```bash
  shasum -a 256 test-results/screenshots/*.png   # every hash must be unique
  ```
- Post with `scripts/post-screenshots.sh <pr> <png-dir> [body.md]`. Never
  `buzz upload`, never a relay media URL (they die behind GitHub's camo proxy).
  Delete superseded screenshot comments after a repost.
- No `SOUL.md` content in a posted screenshot unless the founder approves that
  specific text.

## PR requirements

- Target **`Nuncio-hq/crew`** only. Never `block/buzz` (D-020).
- Every commit signed off (`git commit -s`) — the DCO check fails otherwise.
- PR body lists each upstream-file edit with its justification and actual line
  count, and states whether Option A or Option B was used for the capability
  descriptor.
- `docs/crew/STATE.md` updated in this same PR (issue #117 anti-drift).
- RED-run output from P02 and the live-probe transcript attached.
- Merge through `NuncioCrew Gate`.

## Exit criteria

All five DoD checkboxes demonstrable from artifacts attached to the PR, with no
reviewer needing to re-run the live probe to believe it.
