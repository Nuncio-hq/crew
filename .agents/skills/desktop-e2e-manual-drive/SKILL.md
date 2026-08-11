---
name: desktop-e2e-manual-drive
description: Drive the Buzz/Crew desktop app by hand (computer-use or a human) against the E2E mock bridge, so UI flows that need real clicking — dialogs, editors, failure paths — can be verified and screenshotted without a real backend, relay, or external CLI.
---

# Manually driving the desktop app against the E2E mock bridge

The desktop app **cannot render in a plain browser** — it needs the Tauri IPC
mock installed by `desktop/src/testing/e2eBridge.ts`. `just desktop-screenshot`
handles one-shot captures, but it cannot do multi-step interactive flows
(type → save → close → reopen → assert a re-read). For those, hold a headed
Playwright browser open and drive it yourself.

## Build (this is the step people get wrong)

```bash
. ./bin/activate-hermit          # pinned Node/pnpm; do this first, always
cd desktop
pnpm --filter buzz build:e2e     # NEVER a plain `pnpm build`
```

A plain `pnpm build` strips the mock bridge. Every spec then fails in ways that
look like product bugs (blank dialogs, missing fields). If assertions fail
oddly, re-check which build produced `desktop/dist`.

Kill any stale server on the preview port first — it will happily serve an old
bundle and you will "reproduce" bugs that no longer exist. `lsof` may not exist
on the box; use `fuser -k 4173/tcp` or `pkill -f "http.server 4173"`, then
confirm with `curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:4173/`.

## The headed manual-driving harness

Playwright's config already starts a server on `BUZZ_E2E_PORT ?? 4173` with
`reuseExistingServer`. Add a **temporary, uncommitted** spec that seeds the mock
and then never finishes, so the window stays open:

```ts
// desktop/tests/e2e/zz-drive.spec.ts  — TEMP, do not commit
import { expect, test } from "@playwright/test";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

test("drive", async ({ page }) => {
  test.setTimeout(0);                       // required: no timeout
  await page.setViewportSize({ width: 1500, height: 940 });
  await installMockBridge(page, { /* seed options, see below */ });
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByTestId("open-agents-view").click();
  // ...navigate to the surface under test...
  console.log("READY");                     // watch for this in the log
  await new Promise(() => {});               // hold the browser open
});
```

Run it headed, in the background, and select the scenario with an env var so one
file can seed several states (e.g. a normal run vs a write-failure run):

```bash
cd desktop
DRIVE_SCENARIO=... nohup pnpm exec playwright test zz-drive.spec.ts \
  --headed --workers=1 > /tmp/drive.log 2>&1 &
# wait for "READY" in /tmp/drive.log before interacting
```

If the spec file isn't picked up, check `playwright.config.ts` — projects use
`testMatch` allow-lists, so a new filename may need a temporary entry. Revert
both the config edit and the spec before handing off:

```bash
git checkout -- desktop/playwright.config.ts
rm -f desktop/tests/e2e/zz-drive.spec.ts
git status --porcelain     # must be empty
```

Node cannot run `tests/helpers/bridge.ts` directly (extensionless TS imports
fail with `ERR_MODULE_NOT_FOUND`) — always go through Playwright, not a bare
`node script.mjs`.

## Window handling for recordings

The harness opens "Google Chrome for Testing". Find and maximize it:

```bash
DISPLAY=:0 wmctrl -l
DISPLAY=:0 wmctrl -a "Google Chrome for Testing"
DISPLAY=:0 wmctrl -r :ACTIVE: -b add,maximized_vert,maximized_horz
```

Close stray Chrome new-tab windows first, or computer-use may read the DOM of
the wrong window. Note that `browser_console` / `read_dom` generally will **not**
attach to the Playwright-launched Chromium ("Could not connect to Chrome via
CDP") — rely on screenshots and `zoom` instead, which is what you want for
visual evidence anyway.

## Seeding mock state

`installMockBridge` options live in `desktop/tests/helpers/bridge.ts`; the
handlers are in `desktop/src/testing/e2eBridge.ts`. Grep the bridge for the IPC
command name to learn exactly what a mock returns before blaming the UI. For
Hermes profile work the useful options are:

- `hermesProfiles: ["scout"]`
- `hermesProfileConfigs: { scout: { provider, model } }`
- `hermesProfileSouls: { scout: "# persona\n..." }`
- `hermesProfileWriteFailure: { status: "rejected" | "failed", message }` —
  injects a classified failure into **both** `write_hermes_profile_model` and
  `write_hermes_profile_soul`, which is how you test failure paths without a
  real `hermes` binary.
- `managedAgents: [{ pubkey, name, status, channelNames, runtime, hermesProfile }]`
  — use `TEST_IDENTITIES.*.pubkey` for valid keys, and seed a non-Hermes agent
  (e.g. `runtime: "claude"`) alongside so regression checks are one click away.

Failure seeds are applied at bridge install time, so switching between the
happy path and the failure path means **restarting the harness**, not toggling
something in the UI.

## Gotchas that cost real time

- A click that "does nothing" is usually a near-miss on a button whose position
  shifted after a scroll or a re-render. Re-screenshot and re-aim before
  concluding the feature is broken — a missing error message and an un-clicked
  button look identical.
- Creating an agent from the create dialog may not submit under the mock
  (required fields like LLM provider gate the button). Prefer proving
  save-related behavior through the **edit** dialog, which does persist.
- A newly created mock profile has no SOUL.md, so persona editors may render a
  "has no SOUL.md" error branch instead of an empty textarea. Check whether the
  behavior you see is the mock's missing-state or genuine product behavior
  before reporting it.

## Devin Secrets Needed

None — the mock bridge needs no relay, credentials, or external CLI.
