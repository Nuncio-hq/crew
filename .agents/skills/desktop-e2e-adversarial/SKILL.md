---
name: desktop-e2e-adversarial
description: >
  Drive the desktop Tauri app for adversarial/manual-style feature testing using the
  E2E mock bridge and a temporary headed Playwright spec (recordable on DISPLAY=:0).
version: 1
---

# Adversarial desktop testing via the E2E mock bridge

The desktop app cannot render in a plain browser — it needs the mock bridge
(`desktop/src/testing/e2eBridge.ts`). For exploratory/adversarial testing (beyond
re-running the committed specs), drive it with a *temporary* headed Playwright
spec, then delete the temp files.

## Setup

```bash
. ./bin/activate-hermit                 # pinned Node 24.15.0 / pnpm 11.4.0
pnpm --filter buzz build:e2e            # ~1-2 min; writes desktop/dist
```

`desktop/playwright.config.ts` already declares a `webServer`
(`python3 -m http.server 4173 -d dist`, baseURL `http://127.0.0.1:4173`), so a
temporary config that copies its `webServer`/`baseURL` block will start/reuse the
server for you.

Temporary config for a *visible* (recordable) run — do not use `xvfb-run` when the
run needs to be screen-recorded; use `DISPLAY=:0` plus:

```ts
use: {
  ...devices["Desktop Chrome"],
  viewport: { width: 1560, height: 1100 },
  headless: false,
  launchOptions: { slowMo: 250, args: ["--window-position=0,0"] },
},
projects: [{ name: "adversarial", testMatch: ["**/tmp-*.spec.ts"] }],
```

Iterate selectors headless first (`xvfb-run -a pnpm exec playwright test -c <cfg>`),
then re-run each test with `-g "<name>"` on `DISPLAY=:0` so you can drop
`annotate_recording` markers between tests.

## Gotchas that cost time

- **`page.goto("/")` / `page.reload()` re-installs the mock bridge and wipes its
  in-memory stores** (e.g. the Hermes profile-archive list). If a test performs an
  action and then verifies its persisted effect, navigate *within* the SPA instead
  of reloading. Reloading is fine only when you deliberately want a clean state.
- Accessible names are not always what the visible text suggests. Verified names in
  the Agents view: card start control = `Start Agent`, agent delete confirm =
  `Delete agent`, persona ⋮ trigger = `Open actions for <Persona Name>`.
  Grab real names from `test-results/**/error-context.md` (Playwright writes an
  accessibility snapshot there on failure) rather than guessing.
- Narrow-layout affordances in the Agents view are gated by a **container query**
  on `agents-page-content` (`[@container(max-width:40rem)]:hidden`), not the
  viewport. A 700px viewport is still "wide" because the container is wider than
  the breakpoint minus sidebar; use ~560px to force the ⋮ menu variant.
- Channels are opened by test id: `page.getByTestId("channel-general")`.
- Config-nudge cards only render when the message content has a ```buzz:config-nudge
  fence, the message is interactive, **and** the embedded `agent_pubkey` matches the
  trusted author pubkey — emit the mock message authored by the seeded agent itself.
- Mock command behaviour can be perturbed in-page (fault injection) by wrapping
  `window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__` to reject or delay a single command —
  useful for "slow/failing backend must not block the dialog" checks.

## Devin Secrets Needed

None — everything runs locally against the mock bridge (no relay, no real Hermes
binary required).
