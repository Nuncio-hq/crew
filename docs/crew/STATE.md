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

**Primary relay: the founder's dev-server over Tailscale,
`ws://100.86.143.13:3000`** (Buzz Relay 0.2.1, this repo's build; knows
kind 46043). The hosted `wss://lilgroup.communities.buzz.xyz` is
**deprecated** — no longer a target for acceptance; remove it from agents
and the community list once the dev-server relay is confirmed healthy.

Status 2026-09-08: the dev-server relay answers NIP-11 and completes the
WebSocket handshake, then **closes the socket right after sending the
`AUTH` challenge**. No harness has connected since 2026-09-01 04:05 UTC;
15 `buzz-acp` processes started 2026-09-01 are still retrying (566 failed
reconnects on 09-04, 192 on 09-07, 2,328 on 09-08, zero successes).
Founder is checking the relay on the dev-server (identity / auth config
change after 09-01 is the leading suspect). Tracked in
[#338](https://github.com/Nuncio-hq/crew/issues/338).

Stock Buzz relay with Crew kinds (30680 inert, 30623 wiki, 24201 overlay).
Crew migration numbering (0031 inserted; upstream 0031+ shift by one).

## Known gaps

- Dev-server relay rejects harness AUTH since 09-04 (#338); Hermes has not
  been re-verified on it (#337). The harness reconnect loop has no
  terminal state (2k+ attempts/day) — bug, see #338.
- Desktop Smoke / Integration E2E lanes are advisory only (D-032, D-047);
  post-0.5.23 drift tracked in
  [#346](https://github.com/Nuncio-hq/crew/issues/346) (mention-recipients
  composer case first — likely a real bug).
- Observer "turn done" latency 5–10 s under load (pacer; #344 S2, D-075
  item 6).
