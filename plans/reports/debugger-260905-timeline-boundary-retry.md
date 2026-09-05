# Timeline hard-top pagination stall — read-only diagnosis

Status: DONE_WITH_CONCERNS. Concrete retry gap identified in source; causal browser probe delegated to the existing browser tester. No production changes, builds or duplicate test runs.

## Hypothesis and source evidence

A boundary event can be consumed while pagination is temporarily ineligible, without a later retry when eligibility returns.

- `desktop/src/features/messages/ui/MessageTimeline.tsx:644` creates `loadOlderViaVirtualizer`. It rejects while `isFetchingOlder` or `isHoldingPrepend` (`:654–655`), returning false. Those flags are callback dependencies, but changing the callback does not invoke it.
- `desktop/src/features/messages/ui/useSettleGatedPrependMessages.ts:129` starts the settle watcher. At quiet/stable geometry or the four-second deadline it updates the admitted refs and dispatches a render (`:159–166`). It does not recheck a blocked pagination demand.
- `desktop/src/features/messages/ui/TimelineMessageList.tsx:752` defines `handleScroll`; only a virtualizer scroll event at offset <=200 calls `onStartReached` (`:768–770`). There is no readiness-change retry. The nonvirtual sentinel fallback is disabled by `MessageTimeline.tsx:674` for the active virtualized path.
- `desktop/tests/e2e/release-smoke.spec.ts:150` emits upward wheel input, collects mounted rows, then breaks when the actual scrollTop is zero (`:154–157`). Repeating upward wheel at a hard top can produce no scroll event. Waiting or changing React callback identity cannot generate the missing edge.

Plausible sequence: last actual top scroll occurs during an in-flight fetch or held prepend; callback returns false; new snapshot is admitted and fetching clears while offset remains zero; subsequent upward wheel cannot move the element and never calls the now-eligible callback. This fits a stable zero offset and unchanged continuation count, but flags and cursor evidence must confirm it.

## Discriminating browser probe

At the reproduced stall, record `__CREW_TIMELINE_PAGING_PROBE__`, query-store tail hasMore/cursor, scrollTop and continuation count. Wait more than 4.2 seconds without input and record again. Then move down roughly 80px and back to zero and record again.

- Stable hasOlder=true, fetching=false, hold=false, no callback/count change during waiting or upward-at-top input, followed by new callback/fetch/continuation after the nudge: missing eligibility/boundary invalidation confirmed.
- Callback/fetch counts rise but continuation stays unchanged: inspect `useFetchOlderMessages.ts:52` and `pageOlderMessages.ts:36` early returns and cursor retention instead.
- Holding remains true after the deadline: inspect effect lifetime/ref identity and requestAnimationFrame delivery; do not attribute the stall to missing retry yet.
- Tail hasMore=false: verify server-window fixture/cursor exhaustion before changing UI pagination.

Probe sent to the active browser tester and coordinator. No independent browser session was opened.

## Preserve verified protections

Do not remove the settle gate or its `isHoldingPrepend` fetch guard. They protect WKWebView momentum and prevent cascading requests against an uncommitted boundary. `useUpwardPaginationWheel.ts:28–35` suppresses momentum and releases after an 80ms quiet window; it does not itself fetch or retry. Its suppression lifetime is a separate probe if scrolling away from zero also fails.

A fix should remember a rejected, reader-requested top-boundary demand and re-evaluate it after eligibility clears, while confirming the actual current offset is still within the trigger band. Clear demand after a successful fetch, navigation/search changes, exhaustion or moving away; never treat initial measurement/layout as reader intent. Rechecking only because every render produces a new callback would risk uncontrolled auto-pagination and should be avoided.

Existing `useSettleGatedPrependMessages.test.mjs` covers pure snapshot selection, not the mounted async boundary retry. A meaningful regression should exercise a blocked boundary callback followed by hold/fetch completion with no additional scroll event and verify exactly one retry, plus no retry after leaving the top or entering search.

Unresolved: browser discriminator result; no production-causality claim yet.
