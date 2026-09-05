# Layout and workflow smoke reconciliation

Status: all ten assigned specs verified; 76/76 macOS cases passed and the original six specs also passed 38/38 on Linux.

## Source fixes

- `desktop/src/shared/ui/sidebar.tsx`: collapsed offcanvas content now becomes invisible at the end of the existing 200ms slide. Visibility participates in the same transition; scale, opacity, translation, and rail interaction remain unchanged.
- `desktop/src/features/search/ui/TopbarSearch.tsx`: search height accounts for its existing 18vh top offset. Explicit auto/minmax(0,1fr) grid rows keep the 48px input fixed while results shrink and scroll. Height alone was insufficient: programmatic scrolling could also move the outer dialog and hide the input, caught by screenshot inspection and corrected.

## Test reconciliation

- Narrow composer assertions scope to the thread, avoiding a strict-selector collision with the channel composer.
- Sidebar checks retain complete header/content/footer coverage via the inner sidebar wrapper and explicitly validate the intentional 16px resize rail centered on its edge. The shared responsive helper now accepts a Locator or existing test ID.
- Floor navigation uses the application's hash routes. Search waits for the actual dialog before closing; message-menu coverage opens the real More actions control instead of conditionally skipping a nonexistent right-click menu.
- Search at 800x500 verifies outer bounds, final option accessibility after scrolling, actual result overflow/scroll movement, input height 48, input top offset zero, and outer scrollTop zero.
- Settings compare the real Crew Appearance grouping wrapper. Tooltips expect Active just now after the test sends its message. Voice actions target the specific Preview control, avoiding the Devices Preview navigation button.
- Workflow typing differentiates trigger input and step textarea. Linux exposed that the outgoing trigger inspector could still match Message text immediately after adding a step. Geometry checks wait for inspector animation completion.
- Autocomplete visual regression remains enforced, cropped to textarea and popup instead of the entire app. Darwin and Linux baselines were captured on their actual platforms and visually reviewed by root; keyboard cycling, selection, Escape, Tab and caret assertions remain.

## Validation

- Initial six-spec reproduction: 25 passed, 13 failed (`/tmp/planner-smoke4-repro.log`).
- Full six specs on macOS: 38/38 passed in 1.6m (`/tmp/planner-smoke4-verified.log`); subsequent final search grid/header assertions passed (`/tmp/planner-search-layout-check.log`).
- Final exact source in Linux Playwright 1.60.0 noble arm64: 38/38 passed in 2.3m (`/tmp/planner-smoke-linux-full.log`). Results copied to `/tmp/planner-smoke-linux-full-results`.
- Full Linux workflow-controls suite separately passed 12/12 in 38.6s (`/tmp/planner-workflow-linux-complete.log`).
- Biome and diff checks pass; TypeScript log `/tmp/planner-layout-tsc-verified.log`.
- Final bundle: `/tmp/crew-planner-search-layout-dist`. Search visual evidence: `desktop/test-results/responsive-audit/search-window-floor-800x500.png`.

## Previously unrun tail

All 38 cases in workflow-title-stability, workflows, workspace-binding-selector and worktree-storage ran: 37 passed, one animation observation failed (`/tmp/planner-tail-smoke-repro.log`). The remaining test now waits for the previous inspector to be removed and samples the incoming inspector over bounded animation frames, still requiring actual width/opacity/transform movement. Final full tail rerun: 38/38 passed in 1.4m (`/tmp/planner-tail-smoke-final.log`).

Docs impact: minor. Root owns aggregate changelog/verification updates. No staging, commits, pushes, shared-server restarts or external settings changes.

## Complete Linux shard gate

Final frozen frontend built to `/tmp/crew-planner-shard4-final-dist`; current tests, imported source, tsconfig and native-window configuration copied into isolated Linux container `crew-planner-snapshot-linux`. Playwright 1.60.0 noble arm64 runs the unchanged complete smoke shard 4/4: 342 cases, one worker, zero retries, no snapshot updates. Log: `/tmp/planner-shard4-linux-final.log`. Initial collection-only provisioning errors were resolved before any test ran. Final: 341 passed, 1 failed in 15.6m. The sole failure is `virtualization.spec.ts:362`: an anchor moves 348px during a cascade prepend, exceeding the unchanged 5px bound. All ten originally assigned specs pass. Artifacts: `/tmp/planner-shard4-linux-final-results`. The diagnosed pending-acknowledgement bug and browser-probe timing assumptions are resolved in [the cascade report](debugger-260905-virtualization-cascade-linux.md); independent full virtualization passes on macOS and Linux.

Unresolved: coordinator's new-head remote acceptance.
