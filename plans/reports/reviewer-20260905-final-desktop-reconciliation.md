# Independent late desktop review

Status: DONE. No actionable correctness findings in reviewed changes. No code edits.

Reviewed:
- sessionAgingStore.ts: every actual put/removal emits a fresh Map before notifications; prior snapshots retain prior entries. Repeated reads stay referentially stable. useSessionAging consumes this with useSyncExternalStore. New test imports production TypeScript and proves both reference changes and preserved prior contents.
- onboarding-agent-defaults.spec.ts: per-runtime IPC completion gates preserve the real install command/results while deterministically asserting overlapping pending states and independent success/failure. Gate throws if completion is released without an actual pending invocation.
- thread-head-stale-edit.spec.ts: rendered .message-markdown selectors remain scoped by exact message ID. Root/edit share signer; backfill remains explicitly held until fresh head is asserted.
- channels.spec.ts Welcome assertion: rendered channel reference displays general without the source markdown hash; substantive content assertion retained.
- sidebar.tsx: visibility joins the 200ms slide transition, so offcanvas content stays visible during animation then becomes hidden/non-interactive. Existing per-theme intermediate/final animation evidence preserved.
- TopbarSearch.tsx: dialog height bound leaves viewport margin; auto/minmax(0,1fr) preserves input row and confines overflow to results. Reviewed 800x500 screenshot plus bounds, last-option scroll visibility, positive inner scroll, fixed 48px input, and stationary outer-container assertions.

Independent final validation: full desktop pnpm test after all semantic source changes completed successfully: **7032 total, 7031 passed, 1 existing skip, 0 failures, 148 suites, 98.23 seconds**. Log: /tmp/crew-final-virtual-ack-desktop-tests.log. Final source checkpoint: 5d11998fe. Whitespace-only patch normalization was followed by a frozen install and 19/19 installed-artifact regression pass. The official source check and all six guards passed (/tmp/crew-root-final-ack-desktop-check.log).

The [Projects selection review](reviewer-20260905-projects-overview-selection-search.md) records resolution of all three source findings: visible canonical selection ranges, click-time membership checks, and selection preservation on failed discussion. Root mounted search test passes 1/1 in /tmp/crew-root-global-search-test.log. Final mounted project-review coverage passed 40/40 on macOS and first attempt on Linux.

The [Virtua acknowledgment review](reviewer-20260905-virtualization-ack-fix.md) records matching ESM/CJS behavior and installed-code regressions. Complete virtualization suites passed 11/11 on macOS and 11/11 on Linux with zero retries; the focused Linux cascade/reader pair repeated three times passed 6/6. [Parity evidence](tester-260905-final-virtualization-parity.md) and [diagnostic limits](debugger-260905-virtualization-cascade-linux.md) preserve the distinction between the demonstrated installed-code defect and the inferred exact cause of earlier uninstrumented failures.

Scope limits: no cargo changes reviewed; root owns final project implementation review. Browser selections overlap and are not summed.

Additional test-only review: nostr-bind.spec.ts now observes actual deep-link-nostr-bind listener registration before emitting its request. The invoke observer is installed before the mock bridge, forwards original arguments/options/results, tracks resolved listen IDs and unlisten eventId, and preserves the native payload and all consent assertions. This replaces the weaker IPC-function-exists readiness check that could emit before React subscribed. No production changes, skips or bypasses. Full 11-case spec repeated three times passed 33/33 in 34.2s, verified in /tmp/crew-root-nostr-listener.log. The complete Linux 11-case spec then passed with retries zero in 19.5s (/tmp/crew-pool-linux-nostr-ready.log). This confirms the follow-up while preserving the original 329-case shard record of one startup flake.

Unresolved questions: none.
