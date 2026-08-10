# Verification 0010 — Evidence on the thread log: live probes + rendered card evidence

- **Date:** 2026-08-10
- **Issue:** #121 (evidence on the thread log); **PR:** #128
- **Branch / commit:** `devin/1786360062-evidence-thread-log` @ `d9eea32d0`
- **Plan phase:** 09 (live probes + PR evidence)
- **Constraint honoured:** no computer-use anywhere. All evidence is terminal
  text or PNGs from the headless harness (`just desktop-screenshot`,
  Playwright `locator.screenshot()`).

## Boundary exercised

- **Relay half:** real `buzz-relay` on `http://localhost:3000` backed by the
  Docker Postgres 17 + Redis 7 services; real `buzz` CLI built from this branch
  (`cargo build --release -p buzz-cli`). Two real keypairs (owner + agent) and a
  purpose-created channel per probe. No mocks.
- **Desktop half:** the shipped headless E2E harness (`pnpm build:e2e` +
  Playwright, mock bridge). The desktop app cannot render in a plain browser,
  and the harness bridge's `add_reaction` is mock-only
  (`desktop/src/testing/e2eBridge.ts:9528-9571`; relay mode threads only the
  WebSocket/HTTP transports, `:13200-13236`). A desktop Accept click therefore
  **cannot itself publish to the relay** in a headless run, so Probe 1 is
  executed as two halves and reported as such — see Limits.

## RED-first gate

`parseEvidenceKind` (`desktop/src/features/messages/lib/evidenceTag.ts`) was
temporarily stubbed to `return null` (pre-change behaviour), `pnpm --filter buzz
build:e2e` rerun, and the probe spec executed:

```
3 failed
  probe: all four evidence kinds render legible, distinct cards
  probe: owner accept and reject land kind-7 against the evidence event
  probe: before/after card degrades to labelled links when images are blocked
Error: element(s) not found — waiting for getByTestId('evidence-card-test-run')
```

The stub was reverted and the same spec passed, so the assertions bind to the
new renderer rather than incidental markup.

CLI negative control (pre-change wire behaviour): the identical report content
sent **without** `--evidence` produced tags `[["h","<channel>"]]` — no
`crew-evidence` entry.

## Probe 1A — desktop card + owner review round trip (headless harness)

```bash
. ./bin/activate-hermit
pnpm --filter buzz build:e2e
cd desktop && pnpm exec playwright test evidence --project=smoke
```

Result: **10 passed** (3 `evidence-cards.spec.ts`, 3 `evidence-reactions.spec.ts`,
4 probe tests). Asserted:

- `test-run` card shows heading `Test run` with a `Failing` block and a
  `Passing` block from a red→green body; Accept/Reject controls visible to the
  owner.
- Owner Accept → `✅ Accepted` badge, Accept disabled, and the emitted command
  payload is exactly one `add_reaction` with `emoji: "✅"` and
  `eventId === <evidence event id>` (kind-7 targeting the evidence event, not
  some other row).
- Owner Reject → `❌ Rejected`, a second `add_reaction` with `emoji: "❌"` and
  the same `eventId`, and the reply surface opens **rooted on the evidence
  message** (`message-thread-panel` visible containing the evidence body;
  `ChannelPane.tsx:574-575` maps `onReply` to `onOpenThread`).
- Negative control: a `✅` kind-7 from a **third-party** pubkey does not flip
  the card to accepted.

## Probe 1B — real relay round trip + agent read-back

Channel `feabece1-48e5-41c4-980e-37dca849a3b4`; agent `953d3363…`; owner
`e5ebc6cd…`.

```bash
buzz messages send --channel <CH> --content "$(cat report-test-run.md)" --evidence test-run
# {"accepted":true,"event_id":"c53eea073f4916aef03a58b2350a172f031cd8328f19bd748a3aded7f157d338",…}

buzz messages get --channel <CH>
# c53eea07… kind 9 tags [['h','feabece1-…'], ['crew-evidence','test-run']]

buzz reactions add --event c53eea07… --emoji "✅"    # owner key
# {"accepted":true,"event_id":"0974e1250958d6b4389ea19c4eaff2a5a4e17f74e62d17ff256d6394dc81f35b",…}

buzz --format compact reactions get --event c53eea07…   # agent key — DoD checkbox 4
# {"reactions":[{"count":1,"emoji":"✅","pubkeys":["e5ebc6cdb579be112e336cc319b5989b4bb6af11786ea90dbe52b5f08d741b34"]}]}
```

Reject path on a second evidence event `8e823f97…`:

```
buzz reactions add --event 8e823f97… --emoji "❌"  -> accepted:true
buzz --format compact reactions get --event 8e823f97…
# {"reactions":[{"count":1,"emoji":"❌","pubkeys":["e5ebc6cd…"]}]}
follow-up reply tags: [['h','feabece1-…'], ['e','8e823f97…','','reply']]
```

The agent-readable verdict returns the owner's pubkey and count 1 for both
emoji, and the rejection follow-up is parented on the evidence event.

## Probe 2 — UI-change evidence, no computer-use

```bash
just desktop-screenshot --name evidence-before --active-channel engineering --outdir /tmp/probe2
just desktop-screenshot --name evidence-after  --active-channel general     --outdir /tmp/probe2
# dc4fd780…  evidence-before.png
# 55dfbf23…  evidence-after.png      (distinct hashes)

buzz messages send --channel <CH> --content "$(cat report.md)" \
  --file /tmp/probe2/evidence-before.png --file /tmp/probe2/evidence-after.png \
  --evidence before-after-visual
# event 70ac6d21…, tags: h, imeta(before, m image/png), imeta(after, m image/png),
#                        crew-evidence=before-after-visual
```

Render half (headless): the card renders exactly two `img` elements, both with
`naturalWidth > 0`, distinct `src` values, captioned `Before capture` /
`After capture`, and geometrically side by side (same row, before strictly left
of after). Degrade check: with image requests aborted, the same card renders
**0** images and two labelled links whose `href` is the capture URL — the card
still reads sensibly.

Substitution to note: the relay's Blossom GET returns `401 Unauthorized` to an
unauthenticated page, so the render/degrade captures serve the *same* PNGs from
the preview dir (`desktop/dist/probe/`) using the relay's exact content and
`imeta` tag shape. The relay-side upload/tagging was verified separately above.

## Probe 3 — token-discipline spot-check

Line counts of the report bodies as stored on the relay:

| event | kind | lines | ≤ 30? |
|---|---|---|---|
| `c53eea07…` | test-run | 12 | yes |
| `8e823f97…` | test-run | 1 | yes |
| `70ac6d21…` | before-after-visual | 5 | yes |

No work was re-executed solely to capture evidence: the two Probe 2 PNGs were
captured once and reused for the relay upload, the render assertion and the
degrade assertion; the red→green excerpt reuses the RED-gate run's own output.
This is a spot-check only — the ≤ 30-line bound is **not** enforced anywhere in
code (R-5).

## Rendered card screenshots (distinctness gate)

`desktop/test-results/evidence-probe/`, each scoped with `locator.screenshot()`:

```
a705400488796d8dffd9276ced229d1a80237c8d0261f6c741417c1829495e4c  01-test-run-pending.png
621632427813b554b1008a2f4159d365aed7678b7cbdd3e81275030e2cbfa7dc  02-metrics.png
2e84333c2ae5958e61733c7d6cc8062d68bfa0cc2fa5fe0d4692da8c755761f3  03-diff-stat.png
dd488f8b0f3d0b8f8883bcc33c510de794770f5d7308db9ce00c628f7eb4f35c  04-before-after-visual.png
ba32d428003c061ee0eac6c1880071f4ef1ea28664f8ffdf9c38a7913afd0c2a  05-test-run-accepted.png
52251c058322cd57335b22ef4474ebfed071f58baafa6767a0f7a77f2eed1021  06-test-run-rejected.png
252b09f6017ef7dd46b2aec9ff2f522c1f3662182c086b08b33ea5e2e62cdff7  07-before-after-degraded.png
```

All seven hashes unique (7/7). Card text was additionally confirmed to be in the
pixels (not just the DOM) by OCR of each PNG: e.g. `01` reads
`Test run | Failing | Passing | Tests: 1 failed → 1 passed`, `02` reads
`Metrics | before after | 120ms 80ms | delta | -40ms`, `03` reads
`Diff stat | Files: 4 | +42 −17 — Nuncio-hq/crew#128`, `05` reads `✅ Accepted`,
`06` reads `❌ Rejected`, `07` reads `Before/after visual | Before capture
After capture`.

## Temporary scaffolding (reverted)

- New spec `desktop/tests/e2e/evidence-probe-live.spec.ts` plus a one-line
  `testMatch` entry in `desktop/playwright.config.ts` (the `smoke` project's
  `testMatch` is an explicit allow-list, so a new spec file does not run
  otherwise). Both reverted after the run; the diff is preserved at
  `/tmp/plan121/evidence-probe-live.patch`.
- `desktop/dist/probe/*.png` (build output dir, not tracked).

## Limits

- One machine, Linux x86_64, single relay, single-worker Playwright.
- Probe 1 was executed as two halves because the headless harness cannot make a
  desktop click publish to the relay (mock `add_reaction`). The desktop half
  proves the kind-7 emission target and the card's accepted/rejected states; the
  relay half proves the kind-7 actually lands and is agent-readable. A
  single-process "click in the app → event on the relay" chain remains
  unverified and would need either a relay-backed `add_reaction` in the bridge
  or a real Tauri run (which needs computer-use).
- `just ci` was not run in this session; only the evidence specs and the CLI
  probes above were executed.
- The relay used is the shared local dev relay on `:3000`, isolated per probe by
  a freshly created channel rather than a separate database.
