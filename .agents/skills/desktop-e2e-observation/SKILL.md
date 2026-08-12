---
name: desktop-e2e-observation
description: Drive the desktop app through the Playwright E2E mock bridge for ad-hoc, observation-style testing (live messages, thread replies, unread badges, relay recounts) and capture screenshots the committed specs do not.
---

# Desktop E2E observation runs

`just desktop-screenshot` can only inject plain channel messages. When you need
to *observe behaviour over time* — live thread replies, unread badges, read
frontiers, relay recount overlays — write a throwaway Playwright spec and run it
against the E2E mock bridge. The desktop app does not render in a plain browser,
and computer-use against a normal Chrome window will not work.

## Recipe

1. Build once: `. ./bin/activate-hermit && pnpm --filter buzz build:e2e`.
   Rebuild after every source change — the preview server reuses `dist/`.
2. Write `desktop/tests/e2e/tmp-<topic>.spec.ts` (temporary; do not commit).
   Start from an existing spec such as `tests/e2e/thread-unread.spec.ts` for the
   `installMockBridge` + `TEST_IDENTITIES` + `waitForMockLiveSubscription`
   boilerplate.
3. `playwright.config.ts` gates specs by explicit per-project `testMatch` lists,
   so a new file runs in NO project. Add a temporary sibling config:

   ```ts
   import { devices } from "@playwright/test";
   import base from "./playwright.config";
   export default {
     ...base,
     timeout: 180_000, // only if you add long pauses for a recording
     projects: [{
       name: "observed",
       testMatch: ["**/tmp-<topic>.spec.ts"],
       use: { ...devices["Desktop Chrome"], viewport: { width: 1560, height: 1100 } },
     }],
   };
   ```

4. Run: `cd desktop && BUZZ_E2E_PORT=4195 xvfb-run -a pnpm exec playwright test --config=playwright.observed.config.ts`
   (use a non-default port so leftover servers do not collide).
5. For a screen recording, drop `xvfb-run` and run `--headed` with
   `DISPLAY=:0` plus `launchOptions: { slowMo: 250 }` and explicit
   `page.waitForTimeout(...)` pauses at the states you want to narrate. Match the
   viewport to the real display (`DISPLAY=:0 xrandr`) so the app fills the frame.
6. Write screenshots to an absolute path OUTSIDE `desktop/test-results/` —
   Playwright wipes that directory at the start of every later run.
7. Prove the test is strong: run the same temp spec against a pre-fix worktree
   (a second checkout of the base commit, built the same way, on another
   `BUZZ_E2E_PORT`). If it passes there too, the test does not test the change.

## Mock bridge facts worth knowing

- `__BUZZ_E2E_EMIT_MOCK_MESSAGE__({ channelName, content, parentEventId, pubkey,
  createdAt, kind, extraTags })` emits any kind on the live channel
  subscription — replies (`parentEventId`), reactions (`kind: 7`), deletions
  (`kind: 5` + `extraTags: [["e", id]]`), relay overlays (`kind: 39005`).
- An unread reply must be dated strictly after the read frontier snapshot; use
  `now + 60` for "new" and `now - 60` for "already read".
- Addressable overlays (e.g. kind:39005 thread summaries) are last-write-wins by
  `created_at`. The bridge emits its own summary with `created_at = now` on every
  reply, so an injected recount needs an explicitly FUTURE `createdAt` or it is
  silently ignored.
- A live `kind:5` deletion of a *thread reply* appears not to be applied by the
  mock bridge (the reply stays rendered in the thread panel and unread counts do
  not move). Deletion-driven decrements may therefore be unobservable here;
  injecting an authoritative kind:39005 recount with a lower `reply_count` is a
  workaround, and a real relay run is the fallback.
- A non-broadcast thread reply is NOT a channel timeline row: assert
  `[data-message-id=<id>]` count 0 in the channel while the panel is closed.
- Useful selectors: `thread-unread-badge` (text `(N new)`),
  `message-thread-summary`, `[data-thread-head-id="<id>"]`,
  `message-thread-replies` (in-panel rows), `message-thread-panel`,
  `auxiliary-panel-close`.

## Triaging failures

Desktop specs flake under load; re-run any failure serially before believing it.
Confirm whether a failure is pre-existing by running the same spec in a worktree
of the base commit — several suites (`channel-activity-popover`,
`thread-orientation` 03/04, `inbox-edit`, `inbox-reactions`, `messaging`
"thread refetch preserves a live reply", `relay-reconnect` "reconnect backfills
more missed channel messages", `scroll-history` "fast middle-page scroll") have
failed identically on base at times.

## Devin Secrets Needed

None.
