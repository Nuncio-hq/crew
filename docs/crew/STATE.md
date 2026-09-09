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
Thread Workbench route exists but has no door — Focus grain work is
[#344](https://github.com/Nuncio-hq/crew/issues/344) (S1 doors → S2
sub-second done → S3 Tool Pane → S4 reconcile). Tool Pane (PR · Browser · Sim) in thread focus. Crew
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

The [company deployment choice](ARCHITECTURE.md#company-deployment) is the
founder's dev-server over Tailscale. Current endpoint health, installed build,
and client/agent targets have not been reverified by this documentation change.
Historical AUTH-challenge/close observations in
[#338](https://github.com/Nuncio-hq/crew/issues/338) are transport symptoms,
not evidence of an explicit credential denial or a confirmed root cause.

Stock Buzz relay with Crew kinds (30680 inert, 30623 wiki, 24201 overlay).
Crew migration numbering (0031 inserted; upstream 0031+ shift by one).

## Known gaps

- #337 membership acceptance and #338 reconnect/status/receipt acceptance
  remain open. Real staging verification depends on
  [#348](https://github.com/Nuncio-hq/crew/issues/348); fixture results do not
  establish live relay health.
- Desktop Smoke / Integration E2E lanes are advisory only (D-032, D-047);
  post-0.5.23 drift tracked in
  [#346](https://github.com/Nuncio-hq/crew/issues/346) (mention-recipients
  composer case first — likely a real bug).
- Observer "turn done" latency 5–10 s under load (pacer; #344 S2, D-075
  item 6).
