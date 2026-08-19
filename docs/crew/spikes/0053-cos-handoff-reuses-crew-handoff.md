# Spike 0053 — CoS can delegate with existing `crew-handoff`

- **Status:** PASS
- **Date:** 2026-08-19
- **Founder ask:** Chief of Staff is the contact point; it delegates to Dev

## Question

Does “Oscar talks only to CoS; CoS assigns Dev” need a new assignment
kind or a Paperclip-style ticket bus, or do `KIND_ORG_ROSTER` +
`crew-handoff` already express it?

## Decision affected

Whether the CoS slice is (a) sign a roster + teach CoS to emit
`crew-handoff`, or (b) invent a parallel factory / mission protocol.

## Hypothesis

D-060 already gates auto-created work on the manager chain. The missing
piece is product: a signed Founder → CoS → Dev tree, and CoS actually
publishing the tag. Not a new kind.

## Scope

- Code + decision read on `c795b02add75ec3c14967d6de232d2084c283ec8`
- `crates/buzz-core/src/org_roster.rs`
- `crates/buzz-acp/src/lib.rs` (handoff outside chain)
- `docs/crew/DECISIONS.md` D-060
- `docs/crew/HERMES.md` officer loop
- Time: one read pass. No roster published. No live CoS.

## Exclusions

- Factory-as-code file (founder: keep as discussion)
- Multi-day mission (#151 hold)
- Hard capability floors beyond D-044 (sibling)
- Exact “prototype vs feature” byte bound (product, recorded as leftover)

## Pass criteria

1. A parsed `["crew-handoff", executor, goal-digest]` tag exists.
2. Auto-create work only if the author is on `manager-chain(executor)` or
   is the founder.
3. Officer docs already say CoS must link the parent thread.
4. Off-chain handoff is visible conversation, not a silent drop.

## Fail criteria

Handoff identity requires a new kind, or work can be auto-created by a
peer with no roster edge.

## Environment

- Commit: `c795b02add75ec3c14967d6de232d2084c283ec8`
- Auth: none (read-only)

## Method

Read the tag parser, the kickoff gate comments, and D-060. No live
publish.

## Results

1. `CREW_HANDOFF_TAG` / `["crew-handoff", executor, goal-digest]` —
   `crates/buzz-core/src/org_roster.rs` (tolerant parse, extra fields
   ignored).
2. D-060 item 4: top-down handoff auto-creates work only on the manager
   chain; founder skip-level always; non-chain handoffs stay ordinary
   conversation (no silent drop).
3. `buzz-acp` logs `crew-handoff from outside manager chain — conversation
   only` (`crates/buzz-acp/src/lib.rs` near the kickoff gate).
4. Officer loop (`docs/crew/HERMES.md`): handoff to a report uses
   `crew-handoff` and must link the parent thread so the executor can
   read the founder’s words.
5. Founder has not used the shipped Org roster UI (`STATE.md` #198 is
   implemented; adoption is the gap).

## Edge cases observed

- Peer @mention stays flat. Tree constrains **assignment + budget**, not
  who may talk.
- If no 30680 is signed, CoS cannot be in anyone’s manager chain →
  `crew-handoff` will not auto-create work. “Start the company” (sign
  the tiny tree) is a required first UI/ops step, not a protocol gap.
- Founder lock 2026-08-19: CoS **may prototype** small; feature-sized
  work must hand off to Dev. That bound is **not** in the tag schema.
  Prompt + later hard roles must carry it. Do not encode “prototype”
  as a new kind in this slice.

## Limitations

- Did not publish a live 30680 or a live `crew-handoff` on a relay.
- Did not measure whether CoS models actually emit the tag when only
  prompted (spike 0016 says prompts are not a wall).

## Verdict

**PASS.** Reuse `KIND_ORG_ROSTER` + `crew-handoff`. First CoS slice:
founder signs Founder → CoS → Dev, CoS is the intake mention, CoS
emits `crew-handoff` for feature work. No factory.yaml, no ticket bus,
no new assignment kind.

Leftover product question (does not block the slice): how “small
prototype” is detected — CoS self-declares, diff bound, or founder
confirm. Record in the feature plan; do not invent a kind for it.

## Follow-up test contract

RED before implementation:

1. No roster → `crew-handoff` from CoS to Dev does **not** auto-create
   work (conversation only).
2. Signed tree CoS manages Dev → same tag auto-creates / routes as
   today’s kickoff tests already specify.
3. Founder skip-level @Dev still works.

Existing org-hierarchy e2e (`desktop/tests/e2e/org-hierarchy.spec.ts`)
covers the tag shape; add the “no roster” case if missing.

## Cleanup

None. Read-only.
