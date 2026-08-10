---
phase: 09
title: Live probes and Playwright evidence on the PR
status: planned
priority: P1
effort: M
dependencies: ["04", "05", "06", "08"]
---

# Phase 09 — Live probes + PR evidence

Delivers DoD checkbox 6 and the "agent-readable" half of checkbox 4. The
evidence feature's own PR must carry evidence.

## Probe 1 — bugfix report end-to-end

Against an isolated local relay:

1. An agent completes a scripted bugfix task.
2. Its report message carries `crew-evidence: test-run` with a red→green excerpt.
3. The desktop renders the `test-run` card.
4. The owner clicks Accept.
5. A kind-7 `✅` lands on the relay against the evidence event id.
6. The card reflects the accepted state.
7. **The agent reads the verdict back** with the already-shipped command
   (`crates/buzz-cli/src/lib.rs:746-774`):

   ```bash
   buzz --format compact reactions get --event <EVENT_ID>
   ```

   Step 7 is the agent-readable half of DoD checkbox 4. It needs **no new CLI
   work** — only proof that it returns the reaction.

Repeat the Reject path: `❌` lands and the reply composer opens with the evidence
message as parent.

## Probe 2 — UI change, no computer-use

1. An agent makes a small visual change.
2. It captures before/after with the **headless** harness — never "open the app
   and capture":

   ```bash
   just desktop-screenshot --name evidence-before
   just desktop-screenshot --name evidence-after
   ```

3. It sends the report with `--file` for both PNGs and
   `--evidence before-after-visual`.
4. The desktop renders the images side by side.
5. Degrade check: with images blocked, the card still reads sensibly.

## Probe 3 — token discipline spot-check

- Every probe report stays within a small bound — the issue's example is
  **≤ 30 lines of text per report**.
- **No work was re-executed solely to capture evidence.** Confirm from the
  transcript: the artifact existed already.
- This is a spot-check, not an enforced guard (R-5). Report the observed line
  counts; do not claim the bound is enforced.

## Playwright evidence

Specs from phase 03 cover all four card kinds plus reaction states. For the PR:

- Scope each capture with `locator.screenshot()` — a full-page shot of a timeline
  containing every card produces byte-identical PNGs (root `AGENTS.md`).
- Gate on distinctness **before** posting:

  ```bash
  shasum -a 256 test-results/<dir>/*.png   # every hash must be unique
  ```

  Identical hashes mean two shots captured the same state. Fix the spec; do not
  post.
- Post with `./scripts/post-screenshots.sh <pr> <png-dir> <body.md>` using
  `{{filename}}` placeholders. **Never** `buzz upload` or a relay media URL —
  those fail through GitHub's camo proxy.
- On repost, delete the superseded comment so reviewers do not see stale images.

## CI

- `just ci` green.
- The `base_prompt.md` edit carries its `UPSTREAM-SYNC.md` accounting entry in
  the **same** PR.
- Crew's e2e smoke is flaky under load and Rust integration tests do not run in
  the merge gate. Attribute every red shard individually before calling it a
  regression, and never treat one scoped green run as merge authority (R-9).

## Acceptance criteria

- Probes 1-3 executed with recorded output, including the `reactions get` result.
- Four distinct card screenshots with unique `shasum` hashes, posted via
  `post-screenshots.sh`.
- `just ci` green on the final head SHA.
- No computer-use anywhere in the flow.
- Probe results linked from `docs/crew/STATE.md` (phase 08).

## Deliverable

A verification record under `docs/crew/verification/` — next free id is **0007**
(`docs/crew/verification/` currently ends at 0006).

## Risk

Medium. The most likely failure is a screenshot set that looks complete but
contains duplicate states, which reads as "we verified everything" when one card
was never actually captured. The `shasum` gate is mandatory, not advisory.
