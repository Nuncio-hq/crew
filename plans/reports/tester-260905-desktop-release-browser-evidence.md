# Desktop release browser verification

Status: DONE. Worktree `/Users/a1241968/Desktop/Oscar/crew-wt/upstream-0522`; target `desktop-v0.5.22`. No commits, index writes, or pushes by this worker. Root owns release decisions and complete repository gates.

## Harness and integration

- Reconciled released browser specs and mock bridge with Crew routing, thread-scoped Stop receipts, native model Save and persona contracts. Restored missing imported helper definitions and checked test discovery plus undeclared symbol diagnostics.
- Restored released `playwright.release-smoke.config.ts` and E2E-only Bestie feature marker. Both Playwright servers accept an immutable external bundle directory, so full repository production builds cannot silently replace an active E2E build. Release artifacts live outside the per-test output directory.
- Same-second `get_channel_window` remains the released composite timestamp/event-id cursor path. No cursor-order workaround or injected pagination success.
- Browser tests now exercise Crew's actual hash route, forced thread scope, profile-to-definition editing, PR tool selection, narrow focus layout, and text-only zoom.
- First-message DM tests inspect the native HTTP `send_channel_message` transport, with WebSocket fallback for subscribed-channel sends. The test verifies base DM creation, persona preparation, expanded DM creation, publish, and detached start in order. It also verifies failed first-send cleanup and successful retry into the original recipient set.
- Bestie drag verification waits for the shared-layout spring to stop moving before comparing strict subpixel drag deltas. No geometry threshold relaxed.

## Confirmed product regressions found and fixed by root

- Inherited automatic model defaults had disappeared from the editor.
- Removed experimental control still exposed forced thread-scope settings.
- A nested thread heading produced duplicate clickable title wrappers.
- Opening a PR did not select the PR tool tab; narrow focus then collapsed the content column.
- PR comment invalidation repeatedly fetched because the hook depended on a fresh object after reload generation changed. Independent mounted-hook reproduction and regression tests validate the stable identity fix; complete browser comment/check/room-note flow passes.
- Remote-agent provenance disappeared during the member row's name-to-public-key transition on hover/focus. Marker now remains visible and the browser verifies both states.
- Deep history could remain stuck at the upper boundary while `hasMore` stayed true. Root added guarded retry on further upward wheel input; final clean live-relay verification passes.

## Browser evidence

- Final broad focused batch: 38 selected cases; 35 passed initially. The three remaining cases now pass on targeted reruns after correcting transport/inline-display assertions and waiting for the Bestie layout spring.
- Three final visual scenarios passed together: duplicate-agent exact selection/provenance; complete thread workspace/handoff/isolation; PR comment, checks, rerun and narrow toggle. Log `/tmp/crew-e2e-visual-final.log`.
- All 10 edit-agent scenarios passed earlier, including persisted name/model Save, custom command visibility, missing credentials, baked/global defaults, and persona-linked profile Edit. Final saved-name screenshot rerun passed; `/tmp/crew-e2e-agent-final.log`.
- Seven Stop/model browser scenarios passed; six workspace scenarios passed; three text-zoom/traffic-light clearance cases passed; five composer masking/growth/reduced-motion cases passed.
- Owned tests/config validation: Biome checked 228 files clean; Playwright discovers 1,632 tests across 198 files. Diagnostic TypeScript check found no undeclared names, duplicate declarations, or syntax errors; this supplemental test-tree diagnostic is not a claim that the repository types all E2E fixtures.

## Real infrastructure gate

Dedicated recovery Docker context, scratch PostgreSQL database and owned Redis DB 15; no product database reset. Runner compiles current relay and uses native relay queries through the released browser bridge. Scratch resources are removed by the runner.

- Pre-fix uninstrumented failures: 832–861 of 10,000 reachable, two continuation requests, no duplicate rows, ordering errors, or render-pending timeouts. Authoritative pages had advancing same-second cursor ids and `hasMore: true`, but upward scrolling at zero stopped requesting pages.
- A diagnostic build reached all 10,000 without a recovery nudge; the clean build reproduced the stall. Neither diagnostic success nor the failed clean run is used as the final gate.
- **Final clean gate: 3/3 passed in 4.2 minutes**, after the guarded wheel retry. All 10,000 ids reachable with exact hash match; 199 continuations across 200 pages; final `hasMore=false`, `nextCursor=null`; initial/max/final mounted rows 50/164/114; zero duplicates, ordering violations, or render-pending timeouts. Live overlay/aux both empty at completion. No temporary probes or recovery nudge. Log `/tmp/crew-isolated-release-smoke-paging-fixed.log`; artifacts `/tmp/crew-release-smoke-paging-fixed-artifacts/`.

## PR screenshots

Stable directory `/tmp/crew-upstream-0522-pr-evidence` contains four visually inspected, distinct-hash PNGs: saved agent editor, persistent focused provenance, usable narrow PR hub, and isolated thread workspace. All were captured from passing tests after `waitForAnimations`. `manifest.json` records exact source paths and SHA-256; `body.md` is ready for `scripts/post-screenshots.sh` once root has a PR number. No images uploaded by this worker.

Docs impact: minor; this report. Unresolved questions: none for this slice. Root owns complete repository CI, final release commit/PR, and screenshot publication.
