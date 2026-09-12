# Crew state

Short, current, rewritten in place. History is `git log`, merged PRs, and
[`archive/`](archive/); do not append progress notes.

## Upstream

Pinned to Buzz `desktop-v0.5.23` (`b9392d9d`) by the 2026-09-07 merge;
676 upstream-owned files in 41 CI-checked areas ([`fork-delta.json`](fork-delta.json)).
Next sync is one release delta; see [`FORK.md`](FORK.md).

## Agent kernel (`crates/buzz-acp`)

Crew is ahead of upstream with the session ledger, resume-first recovery,
worktree leases, Cowork, elicitation, receipts, subscription/turn recovery,
governor, desktop-control env, lazy slots, and Hermes tier-1. #7332/#7335 are
cherry-picked; #6732/#7337 remain unadopted pending a decision. ACP AUTH
fencing, reconnect bursts, native diagnostics, bounded storage, and pair-scoped
retry are shipped. Staging and turn/receipt acceptance remain #338 gates.

## Desktop

Channel-first IA (Inbox, channels, and DMs; no Projects/Workbench/Org nav).
Workbench has no door; Focus is [#354](https://github.com/Nuncio-hq/crew/issues/354),
observer completion is [#352](https://github.com/Nuncio-hq/crew/issues/352). Tool Pane,
Crew Dark, text-only zoom, the 1000-line ratchet, and composer-revision mention
flow are shipped. Agent membership is unknown unless runtime nonce, transport,
and community scope match.
Project Wiki Read, bounded full-body Search, and revision-bound Source view are
shipped in [#397](https://github.com/Nuncio-hq/crew/pull/397). Native generation/
publication, Ask/History, and installed staging acceptance remain separate
gates (#348/#362/#363/#366/#367).
The reader remembers page and scroll position only within the captured
owner/community, Project, repository, and door. Source controls require a
current native grant and an exact recorded page reference; the source list and
Markdown citations open a dismissible verified-source pane, while unavailable
source evidence leaves the control disabled instead of opening checkout bytes.
The Project/Wiki Ask composer is visible but disabled with a plain unavailable
message; its former sample answer and channel prefill are removed while
#366/#367's private runtime, history, storage, and dispatch gates remain open.

## Hermes

Profile-per-agent runtime; hire, bind, and offboard from Crew. `default` is
bound after confirmation (D-073). Runbook: [`HERMES.md`](HERMES.md).

## Mobile

Flutter client continues the same company (Need you, threads); kinds mirror
desktop. No org/wiki UI. 1000-line policy.

## Relay / DB

The [company deployment choice](ARCHITECTURE.md#company-deployment) is the
founder's dev-server over Tailscale. Owned #348 snapshot staging is reachable
at `ws://100.86.143.13:3348`, uses isolated `crew_staging_348_snapshot`, and
enforces NIP-42 plus NIP-43. Admission denial, admin readback, and kind `30177`
writes are verified; the daily relay and app-data tree were not used. This is
separate from the historical AUTH challenge/close symptoms in [#338](https://github.com/Nuncio-hq/crew/issues/338),
which still have no established transport root cause.

Stock Buzz relay with Crew kinds (30680 inert, 30623 wiki, 24201 overlay).
Crew migration numbering (0031 inserted; upstream 0031+ shift by one).

## Known gaps

- #337 projection checks and NIP-43 admission/readback are covered. Full #348
  staging UI/native acceptance and #338 reconnect/status/receipt acceptance
  remain open; fixtures do not establish live relay health.
- Desktop Smoke/Integration E2E remains advisory (D-032, D-047); post-0.5.23
  drift is tracked in [#346](https://github.com/Nuncio-hq/crew/issues/346).
- Observer priority preserves causal order and two-urgent/one-normal fairness
  (#352, D-075 item 6); staging has not remeasured the historical 5–10 s
  under-load symptom. See [scheduling and control limits](ARCHITECTURE.md#observer-completion-scheduling-352).
