# Hermes first-class runtime (profile-per-agent)

- **Status:** In progress — Slices 0–1 + Phase 01 (tier-1) delivered; Phase 02 next
- **Date:** 2026-08-05
- **Feature:** [`docs/crew/features/0001-hermes-first-class-runtime.md`](../../docs/crew/features/0001-hermes-first-class-runtime.md)
- **Decision:** D-019 / D-020
- **Branch:** `feat/hermes-first-class-runtime`

## Goal

Make Hermes the primary supported runtime for Crew agents on the
principle **an agent is a Hermes profile; Crew is the office it works
in** — profile owns model/memory/skills/credentials; Crew owns relay
identity, placement, and scheduling.

## Scope

1. [x] Slice 0 — five spikes (records 0009–0013), all conclusive.
2. [x] Slice 1 — manual profile-bound agents, verified live
   ([verification 0006](../../docs/crew/verification/0006-hermes-slice1-live-roundtrip.md)):
   C-02 relay round-trip and strict C-07 model-change-without-respawn.
   Docs-only; runbook [`docs/crew/HERMES.md`](../../docs/crew/HERMES.md).
3. [x] [Phase 01 — Hermes tier-1 runtime entry in Crew](phase-01-upstream-tier1-pr.md)
   — landed via [PR #54](https://github.com/Nuncio-hq/crew/pull/54)
   (D-020: Crew, not block/buzz).
4. [ ] [Phase 02 — Crew UI: binding, readiness, no-model guard (Slice 2)](phase-02-crew-binding-ui.md)
5. [ ] [Phase 03 — profile lifecycle completion (Slice 4)](phase-03-profile-lifecycle.md)

Phase 01 preceded Phase 02 so the tier-1 entry could remove manual
steps (bare-`hermes` probe, automatic `buzz-dev-mcp`, declarative env
guard) before the binding UI.

## Locked boundaries (D-019)

- 1 agent ↔ 1 named profile; manager's `~/.hermes` never bound.
- Model/provider read-only in Crew; no `BUZZ_ACP_MODEL` from any layer.
- Spawn basename `hermes`/`hermes-acp` only; profile via `-p` args.
- Tier-1 promotion lands upstream, not as permanent fork drift.
- Profile create/delete only as explicit manager actions (`-y` on
  delete; verify by directory absence).
- Public agents blocked on the credential-fallback caveat (spike 0010)
  until a Hermes-side isolation switch exists.

## External asks (tracked, not gating)

- Hermes: headless auth probe (`auth status --check`, exit-code
  semantics) — unblocks `auth_probe_args` and full C-12.
- Hermes: per-profile opt-out of global-root credential fallback —
  unblocks public agents.

## Dependencies

- Spikes 0009–0013, verification 0006 (this branch).
- Upstream contributor guide: `AGENTS.md` § Adding a preset / BYOH.
