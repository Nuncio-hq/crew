# Feature 0002 — Attention (Continue) + CoS contact point

- **Status:** PROPOSED
- **Date:** 2026-08-19
- **Founder locks:** Continue + CoS first; CoS may prototype small, must
  hand off feature work; factory-as-code stays discussion; mobile later
  (Flutter first; React Native is not the default)

## Stories

1. Oscar opens Inbox and sees every session that needs him, is sleeping,
   failed, or working. One action: Answer / Wake / Inspect. Wake on
   thread A does not resume thread B.
2. Oscar @CoS only. CoS investigates, may prototype, and `crew-handoff`s
   feature work to Dev. Reports stay in the room.

## Decisions this feature must not reopen

- Channel-first, no board-as-home (D-037)
- No Workbench place (D-065)
- Roster + `crew-handoff` are the assignment plane (D-060)
- Thin fork; Buzz contracts first (D-001, D-025)
- Flutter mobile, not an RN rewrite, unless a later decision supersedes
  FOUNDER-PRODUCT

## Spikes

| Spike | Question | Verdict |
| ----- | -------- | ------- |
| [0052](../spikes/0052-inbox-sleeping-continue-without-new-kind.md) | Continue without a new kind? | PASS |
| [0053](../spikes/0053-cos-handoff-reuses-crew-handoff.md) | CoS delegate without a new kind? | PASS |

## Slices (draft, not approved to implement)

1. **Inbox sleeping rows + Wake copy** — invert the sleep exclude; RED
   contracts from spike 0052. No new kind.
2. **Continue = mention in that thread** — reuse wake-on-mention; prove
   A≠B session ids.
3. **Sign the tiny company** — founder publishes 30680 Founder → CoS →
   Dev (or a one-click helper that still waits for founder sign).
4. **CoS intake prompt** — Layer-2 / CoS definition: feature work must
   `crew-handoff`; prototype allowed; scale/risk paragraph before “done”
   (agent-quality ask, not an app rewrite).

Implementation starts only after RED tests for slice 1 and founder
approval of that slice (D-008).

## Out of scope

- Warp `factory.yaml` as live truth
- #102 / #151 mission machine
- Mobile Need you (reuse Continue contract later)
- React Native client
