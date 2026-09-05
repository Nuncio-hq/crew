# Linux cascade viewport reconciliation

Status: DONE. Independent patch review completed with no findings.

## Concrete failures and evidence

- Complete frozen Linux smoke shard 4/4: 341 passed, one failure in 15.6m, retries zero. `virtualization.spec.ts` maxDrift 348 exceeded the unchanged 5px bound. Log `/tmp/planner-shard4-linux-final.log`; artifacts `/tmp/planner-shard4-linux-final-results`.
- Test-only frame diagnostics reproduced 392px stable viewport loss: initial prepend compensation was correct, then scrollHeight shrank 436 while scrollTop corrected only 44. Exact record `/tmp/planner-cascade-failing-anchor-frames.json`; artifacts `/tmp/planner-cascade-diagnostic-results`.
- Gating the actual older-page reply did not erase the defect: a later run recorded 368px loss. Initial prepend write 5839 was acknowledged; successive resize writes 5471 and 5775 occurred before the next native scroll event. Last wheel was over 400ms earlier. The second write used the old 5839 baseline, losing the first 368px correction. Evidence `/tmp/planner-cascade-gated-errors.txt` and `/tmp/planner-cascade-gated-trace-results`.
- Independent installed-code reproducer executes Virtua's real store, DOM scroller and pending scroll-observer timeout. Delivering that timeout after a prepend write but before its native acknowledgement reproduces exactly 392px loss: resize writes 408 then 756 instead of 408 then 364. An empty intervening layout flush also remains covered. This demonstrates the correctness bug directly. Attribution of the original browser failure to that particular stale timeout remains an inference; the original browser did not capture its internal mode transition.
- Separate observer bugs were also proven: fixed 120ms upward sampling captured a legitimate 5760px prepend before the first wheel arrived; fixed 400ms downward sampling observed only the first 120px wheel. Those were driver scheduling assumptions, not product regressions.

## Source fix

The existing `patches/virtua@0.49.3.patch` now tracks pending shift acknowledgement separately from the transient flushed-jump field, which an empty flush can clear. A stale scroll-end timeout cannot retire shift ownership before the native scroll acknowledgement. Actual native scroll, wheel/manual intent and append ownership still release the appropriate state. Ctrl+wheel remains ignored by the wheel-ownership path.

The existing DOM scroller synchronously acknowledges shift corrections that leave the physical offset unchanged. This covers fitting, exact-fit, clamped and subpixel-rounded writes that generate no native scroll event, preventing pending state from becoming stranded. Both ESM and CJS are changed symmetrically. Existing live-offset correction arithmetic and timers are unchanged.

The patch was whitespace-normalized to match repository conventions. Final pnpm hash: `93df8d370566d17517c10df7dafec0d8e40a443f80c83a5a016bea319c7ed770`.

## Test reconciliation

`virtualization.spec.ts` holds each actual `get_channel_window` response until the upward boundary probe and native wheel input settle, then releases the original payload at the anchor-observation boundary. Overlapping or missing responses fail explicitly. Native burst observation requires all four upward or three downward events, three stable frames and 100ms quiet, while retaining the previous minimum 120/400ms observation windows.

All four 5px bounds, fifteen consecutive pages, Ctrl+wheel, downward exit, actual preload/anchor checks and bounded DOM count remain. The adjacent continued-wheel-during-fetch test 09 is unchanged.

## Validation

- Actual installed ESM/CJS regression before fix: 15 passed, 4 failed, including expected 364 versus actual 756 in both formats. After fix and again after final frozen installation: 19/19 passed. Logs `/tmp/planner-virtua-ack-before.log`, `/tmp/planner-virtua-ack-final-unit.log`.
- Cases cover stale observer timeout, empty flush, native acknowledgement, wheel/manual/append retirement, Ctrl+wheel, fitting/exact-fit/clamped/rounded no-op writes.
- Final frozen-lockfile installation passed (`/tmp/planner-virtua-frozen-install-final.log`); TypeScript passed with empty log (`/tmp/planner-virtua-tsc.log`); Biome and git diff checks passed.
- Independent pool full virtualization: macOS 11/11 in 40.2s and Linux 11/11 in 53.7s, zero retries, failures, flakes or skips. Logs `/tmp/crew-pool-final-virtualization-mac.log` and `/tmp/crew-pool-final-virtualization-linux.log`.
- Independent patch review: no findings. Final aggregate unit suite: 7031 passed plus one existing skip; coordinator's full desktop check passed. Source committed by coordinator as `5d11998fe`.
- Final uninstrumented frontend artifact: `/tmp/crew-planner-virtua-ack-final-dist`.
- Focused Linux 08/09 repeat 3: 6/6 passed in 1.7m, no retries, log `/tmp/planner-virtua-ack-final-focused.log`.

Docs impact: minor. Coordinator/ACP own aggregate changelog and acceptance updates. No Git staging, commit or push performed by this agent.

Unresolved: coordinator's new-head remote acceptance.
