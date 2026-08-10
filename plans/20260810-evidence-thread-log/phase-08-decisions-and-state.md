---
phase: 08
title: DECISIONS.md tag schema and known limit, STATE.md anti-drift
status: planned
priority: P1
effort: S
dependencies: ["04", "05", "06"]
---

# Phase 08 — Decision record + state update

Delivers DoD checkbox 5.

## Why a decision entry

The `crew-evidence` tag is a **sticky wire contract**. Once agents emit it and
the desktop renders it, changing the vocabulary breaks already-published room
history. D-025 requires recording any contract extension in `DECISIONS.md` with
the reason generic Buzz was insufficient.

## `docs/crew/DECISIONS.md` — new entry (next free id, expected D-028)

Must state:

1. **The schema.** `["crew-evidence", "<kind>"]` on existing message kinds, with
   the four values `test-run | metrics | before-after-visual | diff-stat`. First
   occurrence wins. **No new event kind.**
2. **Why a tag and not a new kind.** Existing kinds carry it; other engines and
   clients ignore an unknown tag safely (D-025 "prefer extensions that other ACP
   engines can ignore safely"). Backed by the phase 01 spike verdict — cite it.
3. **Why the CLI appends post-build.** Keeps `buzz-sdk` and `buzz-core` at zero
   additional Crew delta; precedent `crates/buzz-cli/src/client.rs:590`.
4. **The known limit, verbatim in substance from the issue.** Evidence is
   self-reported; numbers and test excerpts *can* be fabricated. This slice
   raises the cost of lying and the odds of getting caught — fabricated numbers
   diverge from CI, fabricated screenshots diverge from the app — it does **not**
   cryptographically verify work. Independent verification stays where it lives
   today: CI and PR review.
5. **The scope limit.** Only the CLI can emit the tag this slice; the desktop
   composer and mobile cannot. Only kind 9 renders a card.
6. **The ✅/❌ reuse** and the fact that `KIND_AGENT_RECEIPT` ignores the tag
   (R-2). If the founder answers D-2 with a different glyph pair, record that
   instead.
7. **Non-enforcement.** The ≤30-line evidence bound is a probe check and a prompt
   rule, not a runtime guard (R-5). Do not write anything implying enforcement.

## `docs/crew/STATE.md` — anti-drift

Issue #117's rule: any PR changing shipped state updates `STATE.md` in the
**same** PR. Update:

- **Current product slice** — evidence-on-thread-log shipped, with what works.
- **Verified evidence** — link the phase 09 probes and the Playwright evidence.
- **Open decisions** — add D-1 (upstream generic half under D-020) and, if still
  unanswered, D-2 (glyph reuse).

State only what is actually shipped and verified. Do not describe phase 07's
draft as an opened upstream PR.

## Other docs

| Doc | Update if |
| --- | --- |
| `docs/crew/UPSTREAM-SYNC.md` | phase 02 created the upstream-file-edit list — confirm it lists every file this slice touched, with final line counts |
| `crates/buzz-cli/TESTING.md` | it enumerates `messages send` flags |
| `desktop/src/features/agents/AGENTS.md` | any harness capability fact changed |

## Acceptance criteria

- The decision entry covers all seven points above and cites the spike.
- `STATE.md` reflects only shipped, verified behavior.
- The known limit appears in `DECISIONS.md`, not only in the issue.
- No doc claims independent verification, enforcement of the token bound, or an
  opened upstream PR.

## Validation

Read each edited doc after writing and check every claim against source, tests
or live state. Verify every link resolves.

## Risk

Low technically, high in drift terms — a wrong or over-claiming decision entry
misleads every future agent that reads it as law.
