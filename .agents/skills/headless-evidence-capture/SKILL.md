---
name: headless-evidence-capture
description: >
  Capture per-component desktop UI evidence and run relay round-trip probes with
  no computer-use: scoped Playwright locator.screenshot() PNGs with a shasum
  distinctness gate, plus buzz CLI probes against a local relay.
version: 1
---

# Headless Evidence Capture (no computer-use)

Companion to the `desktop-screenshot` skill (viewport captures + PR hosting) and
the `sprout-cli` skill (CLI surface). Use this when a PR must be proven with
**one image per component state** and/or a **real relay round trip**, without
opening the app.

## Per-component captures

`just desktop-screenshot` captures a viewport. A full-page shot of a timeline
containing every state yields byte-identical PNGs, so capture each state with
`locator.screenshot({ path })` inside a spec and gate on distinctness:

```bash
. ./bin/activate-hermit
pnpm --filter buzz build:e2e     # ALWAYS first. Skipping it shows up as
                                 # "Cannot read properties of undefined (reading 'invoke')"
cd desktop && pnpm exec playwright test <spec> --project=smoke
shasum -a 256 test-results/<dir>/*.png    # every hash must be unique
```

Identical hashes mean two shots captured the same state — fix the spec, do not
post. To prove text is in the **pixels** and not just the DOM, OCR each PNG
(`sudo apt-get install -y tesseract-ocr`, `pip install pytesseract`, then run it
over a 2x LANCZOS upscale).

## Gotchas that cost time

- **`playwright.config.ts` `testMatch` is an explicit allow-list per project.**
  A brand-new spec file runs in neither project until it is added there. A
  temporary one-line entry (reverted afterwards) is the cheapest route.
- **Scope carefully: the thread panel re-renders the same message**, so
  `getByTestId(...)` can resolve to 2 elements and `locator.screenshot()` fails
  with a strict-mode violation after a reply surface opens. Use `.first()`.
- **Overlays contaminate scoped shots** — a floating "N new messages" pill can
  cover a card header. Emit only the message under test in that spec.
- **Markdown/card layouts often match absolute URLs only** (`/https?:\/\/\S+/`),
  so a relative image `src` silently falls back to plain markdown with zero
  images. Use `http://127.0.0.1:4173/...` and copy fixture PNGs into
  `desktop/dist/` **after** `build:e2e` (the build empties `dist`).
- **Relay Blossom URLs are unusable from the harness page** — an unauthenticated
  GET on `/media/<sha>.png` returns `401`. Verify the relay-side upload and
  `imeta` tags with the CLI, and render from locally served copies; say so in the
  report.
- **Mutating Tauri commands stay mock even in `installRelayBridge` mode** (only
  the WebSocket/HTTP transports are threaded to the relay; see
  `desktop/src/testing/e2eBridge.ts`). A click in the harness cannot publish to a
  real relay. Prove the wire target from `window.__BUZZ_E2E_COMMAND_PAYLOADS__`
  (filter by command name and assert the payload ids), and prove relay landing
  separately with the CLI.
- **Image-degrade states** are reachable with `page.route("**/*.png", r =>
  r.abort())` — good for proving a card still reads sensibly without images.

## RED-first, cheaply

Stub the new parser/helper to its pre-change return value (e.g. a `parseX`
returning `null`), rerun `pnpm --filter buzz build:e2e`, and confirm the spec
fails on the missing `data-testid`. Then revert, rebuild and rerun for GREEN.
This proves the assertions bind to the new renderer, not incidental markup.

## Relay probes with the CLI

```bash
cargo build --release -p buzz-cli          # ./target/release/buzz
export BUZZ_RELAY_URL=http://localhost:3000
```

- A local relay usually already runs on `:3000` with the Docker Postgres/Redis
  services up (`docker ps` → `buzz-postgres`, `buzz-redis`). `just relay` starts
  it otherwise.
- The desktop test identities in `desktop/tests/helpers/bridge.ts`
  (`TEST_IDENTITIES`) work directly as `BUZZ_PRIVATE_KEY`, which keeps CLI and
  harness pubkeys consistent. Isolate a probe by creating a fresh channel
  (`buzz channels create --name … --type stream --visibility open`) and
  `channels add-member` for the agent key.
- Custom event tags survive the relay round trip: verify with
  `buzz messages get --channel <CH>` and read the `tags` array rather than
  trusting the send response.
- Reaction verdicts read back with
  `buzz --format compact reactions get --event <ID>` →
  `{"reactions":[{"count":1,"emoji":"✅","pubkeys":["<owner>"]}]}`.
- Attachments: `buzz messages send --file a.png --file b.png` uploads and appends
  `imeta` tags plus markdown image lines to the content.

## Devin Secrets Needed

None for local relay probes or headless captures. `scripts/post-screenshots.sh`
needs a working `gh auth status` (PR hosting only) — never `buzz upload` or relay
media URLs for PR images, they fail through GitHub's camo proxy.
