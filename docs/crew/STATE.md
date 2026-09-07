# Crew state

Short, current, rewritten in place. One section per surface. History is
`git log`, the merged PRs, and [`archive/`](archive/). Do not append
progress notes here; change the sentence that is no longer true.

## Upstream

Pinned to Buzz `desktop-v0.5.23` (`b9392d9d`) by a real merge
(2026-09-07, branch `sync/upstream-2026-09-07`). Fork delta: 675
upstream-owned files across 41 areas ([`fork-delta.json`](fork-delta.json)),
checked in CI. Next sync is one release delta. See [`FORK.md`](FORK.md).

## Agent kernel (`crates/buzz-acp`)

Crew is ahead of upstream: session ledger + resume-first, thread worktree
leases + Cowork, elicitation, receipts + subscription/turn recovery
(#339/#340), governor / desktop-control env, lazy slot fill (#302), Hermes
tier-1. Upstream #7332 and #7335 are cherry-picked. Upstream #6732
thread-per-session and #7337 busy-owner hold are **not** adopted
(`scope.rs` present, unwired) — needs a decision before any change.
`cargo test -p buzz-acp --lib`: 1185 passing.

## Desktop

Channel-first IA (Inbox + channels + DMs; no Projects/Workbench/Org nav).
Thread Workbench route exists but has no door (D-055 follow-up pending, see
PRODUCT.md "Focus"). Tool Pane (PR · Browser · Sim) in thread focus. Crew
Dark theme, text-only zoom, 1000-line ratchet. Mention send flow follows
upstream's composer-revision security model with Crew context in
`crewSendContext.ts`. Desktop unit suite: 7358 tests passing.

## Hermes

Profile-per-agent runtime; hire / bind / offboard from Crew; `default`
profile bound after confirmation (D-073). Runbook: [`HERMES.md`](HERMES.md).

## Mobile

Flutter client continues the same company (Need you, threads). Kinds
mirror desktop. No org/wiki UI. 1000-line policy.

## Relay / DB

Stock Buzz relay with Crew kinds (30680 inert, 30623 wiki, 24201 overlay).
Crew migration numbering (0031 inserted; upstream 0031+ shift by one).

## Known gaps

- Hosted-relay acceptance for #337/#338 incomplete (receipt kind unknown to
  hosted relay 0.2.1).
- Desktop Smoke / Integration E2E lanes are advisory only (D-032, D-047).
- Observer "turn done" latency 5–10 s under load (pacer, see PRODUCT.md
  Focus, D-075 item 6).
