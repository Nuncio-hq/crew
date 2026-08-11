---
phase: 02
title: Office prompt rule + thin-fork accounting
status: planned
priority: P0
effort: M
dependencies: ["01"]
---

# Phase 02 — Office prompt rule + thin-fork accounting

Delivers the behavioral half of DoD checkbox 1: every agent, on every engine,
learns to attach proportionate evidence when reporting completion.

## Seam

`crates/buzz-acp/src/base_prompt.md` — the office-level prompt embedded for
**every** runtime at `crates/buzz-acp/src/lib.rs:1942`
(`include_str!("base_prompt.md")`), unless the operator passes
`--no-base-prompt` or an override file. This is why the rule goes here and not
into per-agent Layer-3 descriptions, which may legitimately be empty.

Placement: adjacent to `## Communication Patterns` (`base_prompt.md:46`), which
already carries the sibling office rules — callback mentions (`:56-59`) and
"never publish a bare acknowledgement" (`:80`) — and immediately before or after
`## Engineering Discipline` (`:123`), whose line `:133` ("Validate in the shape
the task demands — tests for code, source citations for research, a reproduced
workflow or artifact for UI work") this section makes concrete.

## Files

| File | Change | Budget |
| --- | --- | --- |
| `crates/buzz-acp/src/base_prompt.md` | one new self-contained section | **≤ 18 lines** |
| `crates/buzz-acp/src/lib.rs` | one prompt-assertion test | ~8 lines |
| `docs/crew/UPSTREAM-SYNC.md` | new "Upstream files Crew edits" section | ~15 lines |

`base_prompt.md` currently has **zero Crew delta** (147 lines, identical to
upstream). This is the first Crew edit to it — see the plan's thin-fork table.

## Steps

1. Draft the "Evidence on completion" section. It must be self-contained (one
   heading, no cross-references into other sections) so an upstream sync
   conflict resolves by keeping and re-placing one block. Content:
   - the evidence-defaults mapping from the issue, **compressed to a compact
     list rather than a 7-row table** to hold the line budget;
   - the three token rules — capture in place, text-first, excerpt don't dump;
   - the proportionality escape hatch: when no cheap evidence exists, say what
     is unproven and how to verify — never a decorative screenshot;
   - the no-computer-use constraint;
   - Crew tooling pointers: `just desktop-screenshot`,
     `buzz messages send --file`, `buzz messages send --evidence <kind>`.
2. Add a prompt-assertion test in `crates/buzz-acp/src/lib.rs`, following the
   upstream pattern at `upstream/main:crates/buzz-acp/src/lib.rs:3944`
   (`shared_base_prompt_teaches_portable_agent_drafts`). Assert both that the
   section exists **and that it is at most 18 lines** — the cap is the mitigation
   for R-4 and must be machine-checked, not remembered.
3. Create the "Upstream files Crew edits" section in
   `docs/crew/UPSTREAM-SYNC.md`. The issue instructs recording the edit in this
   list; **the list does not exist yet** (R-7). Seed it with a table of file,
   justification, and resolve hint, covering at minimum the files this slice
   touches. Place it near the existing thin-fork rules (`:20-32`) and the
   conflict policy (`:110-120`).
4. Record for `base_prompt.md`: justification "office-level behavioral rule
   belongs in the office-level prompt"; resolve hint "self-contained Markdown
   section — on conflict, keep it and re-place it after Communication Patterns".

## Acceptance criteria

- The section is ≤18 lines and the test enforces that bound.
- No Crew-specific vocabulary leaks into the *generic* half of the rule — the
  Crew tooling pointers are clearly separated so phase 07's draft can be lifted
  out cleanly.
- The rule mentions no engine by name. It must read identically for Hermes,
  Claude and Codex (D-025).
- `docs/crew/UPSTREAM-SYNC.md` lists `base_prompt.md` with justification and
  resolve hint.
- `cargo test -p buzz-acp` green.

## Validation

```bash
cargo test -p buzz-acp shared_base_prompt
just ci
```

## Anti-drift

If this phase ships in a PR that changes shipped behavior, update
`docs/crew/STATE.md` in the **same** PR (#117).

## Risk

Medium. The prompt is paid on every turn of every agent; an over-long or
preachy section costs tokens forever and contradicts the issue's own frugality
principle. The line cap plus the test is the guard. Rollback is a one-section
revert with no data implications.
