# Plan — #102 Channel-first missions

Spec: [#102](https://github.com/Nuncio-hq/crew/issues/102) — the issue body is the
authoritative spec (Phases 0–4, gates, definition of done, non-goals). This file
is the **execution plan only**.

Status: not started. No branch, no PR.

## Position in the queue

This is the largest of the open issues and the one that most depends on the others
landing first. Recommended order:

```
#109 (revive e2e)  →  #105 / PR #108  →  #104 Phase 01–03  →  #102 Phase 0
```

Reasons, in order of weight:

1. **#102 Phase 2 consumes #105's projections** (`needs_input`, `failed`, retry,
   `Ready to review`). Starting #102 first means re-deriving them and then
   reconciling two implementations of the same concept — exactly the pattern
   `CLAUDE.md` forbids ("do not keep a Crew implementation running beside an
   upstream/existing one that covers the same concept").
2. **Phase 0 is a durable-model spike that touches `crates/buzz-core/src/kind.rs`
   if it needs a new event kind.** Kind allocation and `DECISIONS.md` are
   fork-permanent choices. They should not be made while three other issues are
   still moving the surrounding seams.
3. **Every phase gate in the spec is a runtime behaviour** ("promote → reload →
   same Mission strip", "pause for a user answer, survive restart, resume"). None
   of those are provable by unit tests. They need e2e, and e2e shard 4 is dead
   (#109).

## Hard gate before any UI work

The spec already states it; restating because it is the most skippable step:

> Do not implement the promotion UI until a reload/reconnect can reconstruct a
> Mission from durable test fixtures.

Phase 0 deliverables that must exist first:
- [ ] Durable Mission representation chosen, additive, no parallel client-side
      task database (D-003 / D-010).
- [ ] Decision recorded in `docs/crew/DECISIONS.md` **before** UI implementation.
- [ ] Pure shared projection model with fixtures for every allowed state.

## Crew-specific risks the spec does not cover

- **Upstream-sync cost.** This feature attaches to `MessageThreadPanel.tsx` and the
  thread-root surface — the same files that breach the file-size ratchet (#111) and
  that the v0.5.7 merge silently mangled. Landing a large Crew delta there before
  #111's rule is decided guarantees a painful next sync. Sequence #111's decision
  ahead of #102 Phase 1.
- **New event kind = permanent fork delta.** If Phase 0 concludes a new kind is
  required, that kind lives in an upstream-owned file (`buzz-core/src/kind.rs`) and
  will conflict on every sync. Prefer tags on existing kinds; if a new kind is
  genuinely needed, say why in `DECISIONS.md`.
- **`shadow-panel-left` / in-place panel** is the surface #95 reworked and the one
  ~19 upstream `project-*` e2e specs are currently timing out against (#109).
  Building on it while its test coverage is dark is the main technical risk.

## PR strategy

One PR per spec phase, in order. Phase 0 lands as docs + model + fixtures with no
UI. Do not bundle phases: each gate is a real checkpoint and bundling removes the
ability to revert one.

## Acceptance bar

The spec's Definition of Done (13 numbered scenario steps) is the bar. Additionally:

- **Mutation check** on every behavioural fix.
- Screenshot hash-distinctness check before posting any PR screenshots — identical
  hashes mean the spec captured the same state twice.

## Open questions for Oscar

- Is #102 wanted **before** or **after** #104's Hermes operational phases? Both are
  large; running them concurrently will collide in the thread/Mission surfaces.
- Phase 4 (`Since you left`) is explicitly optional in the spec. Split it into its
  own issue now, or keep it attached?
