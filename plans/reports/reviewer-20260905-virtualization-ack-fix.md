# Virtualization pending-scroll acknowledgment review

Status: DONE. No actionable findings; final browser evidence complete. No implementation edits by this reviewer.

The existing Virtua patch now retains shift ownership while a nonzero physical prepend correction awaits native scroll acknowledgment. This addresses the observed stale-scroll-end race without changing the previously verified live-DOM offset arithmetic.

Reviewed transitions in installed ESM and CJS:
- Nonzero shift flush sets pendingShiftAck. An empty layout flush does not clear it.
- Native ACTION_SCROLL clears it before the unchanged-offset early return. The observer reads the current physical offset when delivering that action.
- ACTION_SCROLL_END is ignored only while both shift mode and pending acknowledgment are active. After acknowledgment, the ordinary scroll-end lifecycle resumes.
- Reader wheel, manual scroll, and non-shift append clear ownership. Ctrl+wheel retains the existing zoom exclusion.
- Shift writes compare actual offsets before/after. Fitting, exact-fit, clamped and subpixel-rounded no-ops acknowledge directly because no native scroll event can follow. Actual movement still waits for native observation.
- ESM and CJS have matching guards, clear paths and no-op handling. The lockfile patch hash 93df8d370566d17517c10df7dafec0d8e40a443f80c83a5a016bea319c7ed770 matches the patch file SHA-256. Both installed artifacts contain the new logic.

Regression quality: virtuaWheelModePatch.test.mjs executes the installed store and DOM scroller extracted from both shipped formats, rather than a mirrored algorithm. It covers stale timer ordering, repeated resize compensation, empty flush, eventual ordinary scroll-end, reader/manual/append escape, Ctrl+wheel, and no-op bounds/rounding. Planner recorded 15 passes/four failures before the fix and 19/19 after, including the reproduced 392-pixel loss in both formats.

Browser test intent: virtualization.spec.ts now counts delivered native wheel events and waits for quiet scroll frames. It holds the real older-page response until the input baseline is settled, preserving the actual IPC result and throwing on overlap/missing gate. The original less-than-five-pixel rollback/drift thresholds, actual prepend observation, multi-page traversal, downward-reader escape and full viewport below-reader assertions remain. The test separates input motion from compensation; it does not suppress product errors or skip failed states.

Scope: installed patch affects Virtua consumers, so full desktop units and focused browser virtualization coverage accompany it. No arithmetic reversal or arbitrary settling delay was introduced into production.

Final validation: full desktop units passed 7031 with one existing skip (7032 total), zero failures, 148 suites, 98.23s; /tmp/crew-final-virtual-ack-desktop-tests.log. Subsequent whitespace-only patch normalization preserved semantics; frozen install and installed-artifact 19/19 regressions passed again (/tmp/planner-virtua-ack-final-unit.log).

Browser validation: complete suite 11/11 macOS (40.2s), 11/11 Linux (53.7s), plus the focused Linux pair repeated three times 6/6 (1.7m); all retries zero. See tester-260905-final-virtualization-parity.md and debugger-260905-virtualization-cascade-linux.md.

Unresolved questions: none.
