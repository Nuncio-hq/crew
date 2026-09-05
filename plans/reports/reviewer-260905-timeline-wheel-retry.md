# Timeline wheel retry review — 2026-09-05

**Status: DONE.** No actionable correctness findings in the narrow fix. Production source was reviewed read-only; reviewer added only the new mounted hook regression file.

## Verified behavior

- `useUpwardPaginationWheel.ts` accepts the existing `onStartReached` callback through a current ref. The wheel listener does not depend on its changing identity, so fetching/hold transitions cannot cancel the pending momentum-release timer and leave suppression latched.
- Negative wheel input at actual scrollTop <=200 invokes that current callback even when the browser emits no scroll event at a hard top. Mount and callback changes do not fetch; retry remains driven by user input.
- `TimelineMessageList.tsx` supplies the existing guarded callback. `MessageTimeline.tsx` still rejects during fetching, prepend hold, search, skeleton or exhausted history. No guard was weakened or bypassed.
- A successful request retains existing momentum suppression for tall timelines and the existing 80ms quiet release. A blocked request cannot arm new suppression, but preserves already-active suppression until quiet or downward input.
- Ctrl+wheel returns before cancelling bottom intent or paging. Downward input clears suppression without requesting history. Away-from-top and short-timeline behavior remain bounded by the existing geometry rules.

## Focused validation

New `desktop/src/features/messages/ui/useUpwardPaginationWheel.test.mjs` mounts the actual hook against a DOM scroll element. **5 tests passed**, zero failures/skips:

1. At scrollTop zero, blocked callback becomes eligible; the next upward wheel starts paging without any scroll event. Mount and callback changes alone never page.
2. Updating callback identity retains the original pending release timer; completing the quiet window releases suppression.
3. A temporarily blocked request preserves active momentum suppression, with downward input clearing it.
4. Ctrl+wheel, downward input and input away from the top never request older history.
5. Short timelines request history without suppressing wheel input.

The DOM timer is controlled for deterministic quiet-window transitions; this verifies timer lifetime and event cancellation, not WKWebView compositor physics. Biome and scoped diff-check passed. Root owns TypeScript/full gates and pool owns real-browser 10k-row acceptance.

Docs impact: review report only. No production edits, commits, index changes or pushes by reviewer.

Final browser confirmation: clean-bundle release smoke passed 3/3, all 10,000 ids with exact hash, 199 continuations and zero duplicate/order/render-pending errors. See [browser report](tester-260905-desktop-release-browser-evidence.md).

Unresolved correctness questions: none.
