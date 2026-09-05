# Renderer integration review — 2026-09-05

Status: DONE_WITH_CONCERNS. No verified production regression found in the requested integration scope. No production or test files edited.

## Scope

Compared current merge against Crew HEAD and desktop-v0.5.22: relayClientSession, relayEventPublisher, liveSubscriptionSetup, presenceRelaySubscription, presence/hooks, markdown.tsx, markdown/buzzPermalinkComponents.tsx and types.ts, HomeView, InboxDetailPane, InboxMessageRow. Followed adjacent ownership, visibility, navigation, send, and audience helpers where required.

## Verified behavior

- Publish captures community ownership before the rate-limit await; checks ownership, generation, and pending operation identity before recovery and retry. Current root fix avoids transport capture on a missing socket. Runtime tests cover initial outage, community switch during gate, and community switch after send failure.
- Presence replacement requires the live subscription open status, emitted by the EOSE/recovery readiness path. Failed/timeout candidate closes; reconciler retains previous subscription until replacement is ready. Live updates preserve failed aggregate snapshots instead of falsely marking sibling authors healthy.
- Extracted permalink renderer retains authored channel/message links through AuthoredDeepLinkAnchor. Unknown destinations require the existing visibility resolver; entity links retain active-relay URL canonicalization and metadata tooltip handling.
- HomeView routes hidden-DM opening through scoped reopen helper; exact returned channel id and generation checks precede navigation. Existing verified mission target remains authoritative for navigation/send. Inbox detail keeps canonical thread root and unresolved audience semantics while propagating released video-review context, reply callback, and edit mention removals.

## Validation

- Relay publication/replay/transport: 59/59 pass. Log: /tmp/crew-independent-relay-review-tests.log.
- Presence/live setup/Markdown/home batch: 210 tests, 204 pass, 6 fail. Log: /tmp/crew-independent-renderer-review-tests.log.
- Four mounted Inbox reopen tests fail before exercising navigation: JSDOM fixture lacks CSS.escape; released useAnchoredScroll.ts:296 references CSS. Pointer-only TAP independently confirms ReferenceError: CSS is not defined. Log: /tmp/crew-inbox-review-tap.log. Root notified; fixture update/retest required. Failed mounts left handles, so only reviewer-owned hanging children were terminated to obtain runner summary.
- Remaining two failures are presentation expectations: project Inbox expects Review while current format returns Pull request; agent mention test expects mention-chip-agent while Crew renders agent-mention-highlight. These do not establish a functional/authorization regression; root already handling presentation/guard reconciliation.

## Limits

Read-only source review plus focused tests; no native visual run or live relay exercised. No new actionable production findings. Root should finish fixture/presentation reconciliation and its full gates before merge.

Unresolved questions: none.
