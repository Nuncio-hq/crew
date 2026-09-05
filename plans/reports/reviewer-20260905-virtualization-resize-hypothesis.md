# Virtualization resize compensation countercheck

Status: historical countercheck, superseded by [the reviewed acknowledgment fix](reviewer-20260905-virtualization-ack-fix.md) and [final investigation](debugger-260905-virtualization-cascade-linux.md). No production edits or browser runs by this reviewer.

The observed height delta -436 with scroll compensation -44 and anchor drift -392 is consistent with two successive resize jump flushes sharing a stale stored scroll offset **only after shift mode has ended**. Planner identified, and this review verified, that the installed JavaScript includes a Crew patch absent from its source maps. Treat the maps below as explanatory upstream source, not the complete installed behavior.

Actual desktop/node_modules/virtua/lib/index.js:287 and patches/virtua@0.49.3.patch:170 write `W((o ? W(u[i], s) : e.C()) + t, s)`: shift mode uses the live physical DOM offset; non-shift mode uses the stored offset. Therefore the existing patch already protects multiple shift-mode resize flushes. Any stale-offset overwrite hypothesis requires an intervening scroll-end or manual-mode transition.

Installed Virtua source map (desktop/node_modules/virtua/lib/index.js.map) contains these relevant original source locations:
- core/store.ts:238 returns the stored scrollOffset directly.
- core/store.ts:243–246 _flushJump returns the accumulated jump and resets it to zero; it does not advance scrollOffset.
- core/store.ts:300 advances scrollOffset only when ACTION_SCROLL arrives.
- core/scroller.ts:157–159 flushes the jump and calls updateScrollOffset.
- core/scroller.ts:350–353 writes viewport scrollTop as store.$getScrollOffset() + jump.

Conditional candidate ordering after shift mode ends: start at stored/DOM offset S; resize flush -392 writes DOM S-392; before the browser dispatches scroll, another resize flush -44 uses unchanged stored S and overwrites DOM to S-44. Heights shrink by -436 but final scroll compensation is only -44, leaving exactly -392 drift. This explains why source instrumentation that changes scheduling could hide the failure. A DOM scrollTop setter/event trace in the existing frozen-bundle test can distinguish this from a single -44 write caused by resize filtering.

Required mode-transition candidate / alternative jump-filter explanation: core/store.ts ACTION_SCROLL_END (320–328) resets shift mode after the 150ms debounce in core/scroller.ts:76–89. Later resize then uses the per-item above-viewport predicate instead of compensating all changed rows. The current frame-level evidence alone cannot determine whether the missing -392 was discarded during jump selection or overwritten during jump application.

Current TimelineMessageList clears its one-render shift flag at 691–700; Virtua's length-change dispatch guard means clearing that prop does not itself reset store shift mode. useUpwardPaginationWheel prevents continuing upward wheel default after paging but does not alter Virtua's private offset accounting. No source-supported reason to blame its 80ms release by itself.

Planner follow-up evidence (/tmp/planner-cascade-gated-errors.txt, gated08): the real channel-window response was held until the initiating wheel burst settled. Native scroll at 6830.9ms established 5839. Resize write at 6848.5ms set 5471 (-368); resize write at 6860.9ms set 5775 (5839 - 64); only at 6887ms did native scroll update to 5775. Height shrank 432; compensation retained only 64. Last wheel was 6328.8ms and last native wheel-scroll 6366ms. This confirms the stale-offset overwrite, and excludes continued user input as its cause in that run. Root and planner received the finding. Planner is tracing shift/scroll-end directly in the frozen bundle to identify the mode transition.

Resolution: deterministic installed store/scroller tests proved that an old scroll-end callback can retire a pending prepend correction and reproduce the loss. The pending acknowledgment guard fixes that demonstrated defect while preserving shift arithmetic. The exact timer ordering of earlier uninstrumented browser failures remains an inference; final macOS/Linux browser suites and focused repeats pass. No unresolved implementation or validation action remains in this countercheck.
