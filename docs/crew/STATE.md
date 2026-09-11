# Crew state

Short, current, rewritten in place. One section per surface. History is
`git log`, the merged PRs, and [`archive/`](archive/). Do not append
progress notes here; change the sentence that is no longer true.

## Upstream

Pinned to Buzz `desktop-v0.5.23` (`b9392d9d`) by a real merge
(2026-09-07, branch `sync/upstream-2026-09-07`). Fork delta: 676
upstream-owned files across 41 areas ([`fork-delta.json`](fork-delta.json)),
checked in CI. Next sync is one release delta. See [`FORK.md`](FORK.md).

## Agent kernel (`crates/buzz-acp`)

Crew is ahead of upstream: session ledger + resume-first, thread worktree
leases + Cowork, elicitation, receipts + subscription/turn recovery
(#339/#340), governor / desktop-control env, lazy slot fill (#302), Hermes
tier-1. Upstream #7332 and #7335 are cherry-picked. Upstream #6732
thread-per-session and #7337 busy-owner hold are **not** adopted
(`scope.rs` present, unwired) — needs a decision before any change.
ACP startup/reconnect AUTH acknowledgements match the exact sent event ID;
unrelated OK/CLOSED frames remain buffered for normal processing (#338 slice 1).
Desktop-managed ACP generations opt into a six-attempt/300-second reconnect
burst, then one probe every 270–330 seconds. Exact AUTH rejection ends the burst;
authentication and subscription recovery must finish before health resets.
Native transport diagnostics are separate from process lifecycle, fenced by
native owner, runtime key, start nonce and leave/rejoin epoch. Missing or expired
records become unknown; exited-generation results are explicitly historical.
Manual retry uses the existing pair-scoped restart. Per-pair diagnostic storage
is bounded; native preflight preserves required history, and ACP rechecks
capacity before writing or recreating a file. Standalone ACP without the
status environment pair retains legacy retry behavior. Installed staging and
in-flight turn/receipt acceptance remain #338 gates, dependent on #348.

## Desktop

Channel-first IA (Inbox + channels + DMs; no Projects/Workbench/Org nav).
Thread Workbench route exists but has no door — Focus presentation is
[#354](https://github.com/Nuncio-hq/crew/issues/354), observer completion
is [#352](https://github.com/Nuncio-hq/crew/issues/352). Tool Pane (PR · Browser · Sim) in thread focus. Crew
Dark theme, text-only zoom, 1000-line ratchet. Mention send flow follows
upstream's composer-revision security model with Crew context in
`crewSendContext.ts`. Agent channel membership remains observer-derived: the
bounded projection requires the matching native runtime `startNonce`, connected
transport, and active community scope before exposing zero/nonzero; otherwise
it stays unknown.
The Project/Wiki Ask composer is visible but disabled with a plain unavailable
message; the former sample answer and channel prefill are removed while
#366/#367's private runtime, history, storage, and dispatch gates remain open.

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

- #337's Desktop/fixture projection checks are covered, while membership
  staging acceptance and #338 reconnect/status/receipt acceptance remain open.
  Real staging verification depends on
  [#348](https://github.com/Nuncio-hq/crew/issues/348); fixture results do not
  establish live relay health.
- Desktop Smoke / Integration E2E lanes are advisory only (D-032, D-047);
  post-0.5.23 drift tracked in
  [#346](https://github.com/Nuncio-hq/crew/issues/346) (mention-recipients
  composer case first — likely a real bug).
- Observer priority preserves causal order and two-urgent/one-normal fairness
  (#352, D-075 item 6). The historical 5–10 s under-load symptom has not been
  remeasured in staging. See [scheduling and control limits](ARCHITECTURE.md#observer-completion-scheduling-352).
