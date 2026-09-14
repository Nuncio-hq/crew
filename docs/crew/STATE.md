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

Shipped: channel-first IA (Inbox + channels + DMs; no Projects/Workbench/Org nav).
Next UI direction: Codex-style sidebar + central conversations (D-078);
[blueprint](../../design/companyos/README.md) is simulated, implementation pending.
Legacy Workbench routes redirect to Inbox or the channel thread (D-065).
Further Focus/layout integration is
[#344](https://github.com/Nuncio-hq/crew/issues/344); its old Workbench-door
plan and latency diagnosis need reconciliation with D-065/D-078 and current batching before implementation. Tool Pane (PR · Browser · Sim) in thread focus. Crew
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

Host inventory verified over SSH on 2026-09-08: daily `buzz-relay` is a
host process on dev-server; PostgreSQL 17, Redis 7 and MinIO run there in
Docker. NIP-11 responds and advertises relay version 0.2.1. Endpoint and
topology: [`ARCHITECTURE.md`](ARCHITECTURE.md). This inventory did not retest
the previously reported harness AUTH/reconnect failure (#338).

Snapshot staging on the same dev-server is agreed (D-077), **not yet
provisioned or verified**. No staging endpoint is assigned. Agents must use
the environment policy in [`TESTING.md`](TESTING.md), not the daily relay
as a fallback for missing test data.

Stock Buzz relay with Crew kinds (30680 inert, 30623 wiki, 24201 overlay).
Crew migration numbering (0031 inserted; upstream 0031+ shift by one).

## Known gaps

- D-076 records the coding delivery loop as the first CompanyOS priority;
  department delegation and end-to-end verified delivery are product targets,
  not claims of shipped behavior. See [`PRODUCT.md`](PRODUCT.md).
- Previously reported dev-server AUTH/reconnect failure (#338) and Hermes
  readiness (#337) still need verification; an NIP-11 response does not prove either.
- Desktop Smoke / Integration E2E lanes are advisory only (D-032, D-047);
  post-0.5.23 drift tracked in
  [#346](https://github.com/Nuncio-hq/crew/issues/346) (mention-recipients
  composer case first — likely a real bug).
- Observer "turn done" latency 5–10 s under load (pacer; #344 S2, D-075
  item 6).
