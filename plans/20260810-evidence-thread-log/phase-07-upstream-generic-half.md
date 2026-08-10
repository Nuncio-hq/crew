---
phase: 07
title: Upstream generic half — BLOCKED on founder decision
status: blocked
priority: P2
effort: S
dependencies: ["02"]
---

# Phase 07 — Upstream generic half (BLOCKED)

This phase exists so DoD checkbox 1's second clause is **visible and undropped**,
not silently executed and not silently deleted.

## The conflict

**Issue #121 item 1 asks:**

> **Parallel, non-blocking:** open an upstream PR to `block/buzz` proposing the
> *generic* half (evidence-proportionate-to-change-type culture, no
> Crew-specific tooling). If accepted, the fork delta shrinks on a future sync.
> Do not wait on it for anything.

**Repo law forbids it:**

- `docs/crew/DECISIONS.md` **D-020** — *"no pull request will be opened against
  `block/buzz` for this feature (or, by default, for any Crew work)."*
- Root `AGENTS.md` — *"Do not propose, draft, or open pull requests against
  `block/buzz`; the upstream remote's push URL is disabled on purpose."*
- `docs/crew/UPSTREAM-SYNC.md:17` — *"The local upstream push URL is
  deliberately disabled. Never push to `block/buzz`."*

Per `docs/crew/IDENTITY.md:41` and AGENT-WORKING-AGREEMENT MUST #5, a conflict
between documents is surfaced, not silently resolved.

**Precedent for the identical collision:**
`plans/20260805-1330-hermes-first-class-runtime/phase-01-upstream-tier1-pr.md`
was retargeted to `Nuncio-hq/crew` with the note that the upstream-targeted
version of that file is historical.

## What this phase delivers while blocked

A **Crew-owned draft artifact only** — no upstream remote interaction of any
kind.

| Deliverable | Detail |
| --- | --- |
| `docs/crew/upstream-proposals/evidence-on-completion.md` (new, Crew-owned) | The generic, Crew-free section text: the evidence-by-change-type culture, the three token rules, the proportionality escape hatch. No `just desktop-screenshot`, no `--evidence`, no `crew-evidence` tag, no NuncioCrew naming |
| Note in the same file | That it is a proposal draft held under D-020, not a submitted PR, with the link to this phase |

Writing the draft costs minutes and keeps the option open at zero risk. Phase 02
should keep the Crew tooling pointers separable precisely so this text lifts out
cleanly.

## Founder decision required (D-1)

Choose one:

- **A — Keep D-020 (default).** Phase 07 ends at the draft. DoD checkbox 1's
  upstream clause is recorded as consciously deferred, not forgotten. No further
  action.
- **B — Scoped exception.** Record a new entry in `docs/crew/DECISIONS.md`
  authorizing this one upstream contribution, stating who submits it and under
  what identity. Only then does anyone touch `block/buzz`.

**Until the founder picks, this phase must not be executed beyond the draft.**
No agent may open, draft-in-GitHub, or push toward `block/buzz`.

## Blocking status

Blocked on D-1. Does **not** block phases 01-06 or 08-09 — the issue itself calls
this track "parallel, non-blocking".

## Acceptance criteria

- The generic draft exists and contains no Crew-specific vocabulary or tooling.
- The D-020 conflict is stated in the draft and in the PR description.
- **No PR, branch, or push exists against `block/buzz`.**
- If the founder chooses A, DoD checkbox 1's upstream clause is explicitly marked
  deferred in the issue, with the reason.

## Risk

Zero technical risk. The real risk is process: an agent reading only the issue
and not the decision log would open an upstream PR and violate D-020. This file
is the guard against that.
