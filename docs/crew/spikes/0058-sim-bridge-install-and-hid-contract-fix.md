# Spike 0058 — sim bridge install + HID contract fix, live re-verify (#246)

- **Status:** PASS (CLI-level live re-verify); Tool-Pane-through-app re-run
  not repeated this session
- **Date:** 2026-08-20
- **Issue:** [#246](https://github.com/Nuncio-hq/crew/issues/246)
- **Related:** [#196](https://github.com/Nuncio-hq/crew/issues/196),
  [#237](https://github.com/Nuncio-hq/crew/issues/237), spike 0028, spike 0057

## Question

Now that `baguette` is installed via Homebrew on this founder Mac, do the
sim-bridge HID commands spike 0057 needed for B1–B3/C3 (boot + tap/swipe/type,
`sim_snapshot` → `sim_tap`) actually work against a live booted simulator with
the exact argument shapes the product code sends?

## Decision affected

Whether #246 closes the FAIL rows B1–B3/C3 from spike 0057, and whether
D-058's baguette → idb_companion discovery ladder needs to change (it does
not — this spike only fixes the HID contract behind it).

## Hypothesis

`brew install baguette` unblocks discovery (B1's `bridge_missing`). Once
installed, `tap`/`swipe` additionally require `--width`/`--height` (missing
in the pre-fix code — a hard CLI error, not a silent no-op), and the pre-fix
`dispatch_hid` sent subcommands (`home`, `rotate`, `text`) that do not exist
in `baguette`'s CLI at all. Both bugs would have surfaced as new FAILs on
B2 even after B1 was fixed.

## Scope

- Same founder Mac as spike 0057 (`Darwin 25.5.0 arm64`, macOS 26.5)
- Same booted iPhone 17 Pro (`05DA0D1B-4E34-4678-80C4-D888624313DA`) spike
  0057's B4 row used
- `baguette` 0.1.92, installed via `brew install baguette` in this session
  (standard Homebrew formula, no network secrets, fully reversible)
- CLI-level verification of the exact argv the fixed
  `resource_governor::bridge::bridge_*_args` builders construct

## Exclusions

- Full Tool-Pane-through-app re-run (`NuncioCrew.app` on a live relay
  channel, `POST /agent-control`) — not repeated this session; that needs
  the packaged `.app` on a live community session, a separate live-Mac
  walkthrough (see Verdict)
- `idb_companion` (not installed here; ladder keeps it as the documented
  fallback per D-058 / spike 0028 — unchanged by this issue)

## Pass criteria

`describe-ui` returns a root `frame` (screen size) plus the node tree; `tap`,
`swipe`, `press`, `type`, `key` each return `{"ok":true,...}` for exactly the
argv the corrected builder functions produce.

## Fail criteria

Any command exits non-zero, or the JSON shape `root_frame_size`/`ax_node`
assume (`agent_control/live.rs`) doesn't match the live payload.

## Method

1. `which baguette` / `baguette --version` — confirm the brew install landed
   on `PATH` (`/opt/homebrew/bin`).
2. `baguette describe-ui --udid <udid>` — capture the root `frame` and a leaf
   node's `frame`/`label`/`role` against `ax_node`'s field lookups.
3. Re-run `tap`, `swipe`, `press home`, `type`, `key Enter` with the exact
   flags `bridge_tap_args` / `bridge_swipe_args` / `bridge_press_args` /
   `bridge_type_args` / `bridge_key_args` emit — including the
   previously-missing `--width`/`--height` on `tap`/`swipe`.
4. Confirm unit tests (`bridge.rs`'s existing table + the new
   `agent_control::live::bridge_parsing_tests`) pin these exact shapes so a
   future refactor can't silently drift from the live contract again.

## Results

### Discovery — PASS

`baguette` 0.1.92 resolved at `/opt/homebrew/bin/baguette` via
`brew install baguette`. `find_command`'s existing Homebrew-path probing
(`/opt/homebrew/bin`, `/usr/local/bin`, login-shell `PATH`) picked it up
without restarting the app — no additional discovery-ladder code was needed
for this half of #246.

### `describe-ui` — PASS

Root object carries `"frame": {"height": 874, "width": 402, "x": 0, "y": 0}`
on this iPhone 17 Pro (iOS 26.5), and `"children"` holds the accessibility
tree; each node uses `"frame"` / `"label"` / `"role"` / `"identifier"` —
matches `root_frame_size` and `ax_node`'s field lookups exactly. Excerpt:
`assets/0058-sim-bridge-install-and-hid-contract-fix/json/describe-ui-root-excerpt.json`.

### HID commands — PASS

| Command | Args (as built by `bridge.rs`) | Result |
|---|---|---|
| tap | `tap --udid <udid> --x 200.0 --y 400.0 --width 402.0 --height 874.0` | `{"ok":true,"action":"tap"}` |
| swipe | `swipe --udid <udid> --start-x 200.0 --start-y 600.0 --end-x 200.0 --end-y 300.0 --width 402.0 --height 874.0` | `{"ok":true,"action":"swipe"}` |
| press | `press --udid <udid> --button home` | `{"ok":true,"action":"press"}` |
| type | `type --udid <udid> --text hello` | `{"ok":true,"action":"type"}` |
| key | `key --udid <udid> --code Enter` | `{"ok":true,"action":"key"}` |

Full transcript:
`assets/0058-sim-bridge-install-and-hid-contract-fix/json/hid-command-results.json`.

Before this issue's fix, `tap`/`swipe` omitted `--width`/`--height` (exit 64,
"Missing expected argument"), and the Sim tab's `dispatch_hid` sent
`home`/`rotate`/`text` as bare subcommands `baguette` does not have.

## Edge cases observed

1. **A newly booted sim needs a settle window.** `describe-ui` against a
   simulator with no frontmost app can return sparse output until
   SpringBoard finishes loading. Not a bridge bug, and out of scope for
   #246 (which is the discovery/HID contract, not boot-readiness timing) —
   noted here for a future spike on `sim_snapshot` retry/backoff right after
   `sim_boot`.
2. **Root frame and node frames share one coordinate space.** The root's
   `frame` is in the same device-point space as every descendant node's
   `frame` (confirmed by the numbers above), so caching it once per
   `sim_snapshot` and reusing it for the next `sim_tap`/`sim_swipe` on the
   same UDID (`LiveHost::screen_sizes`) is correct without a bridge
   round-trip on every HID call.

## Verdict

**PASS** at the CLI contract level. The exact failure spike 0057 hit
(`bridge_missing`, B1) is fixed by installing `baguette`; the failures that
would have hit next (B2: missing `--width`/`--height`, wrong subcommands) are
fixed in `bridge.rs`/`agent_control/live.rs`/`resource_governor/commands.rs`
and CLI-verified live against the same simulator UDID spike 0057 used. B3
(hide-pane stream pause) was blocked purely on B1 and is unblocked
transitively, but not independently exercised here (it exercises pane
visibility timers, not the bridge contract).

Re-running spike 0057's B1–B3/C3 rows **through the installed app**
(`POST /agent-control` against a live relay channel) is not repeated in this
session — that needs the packaged `.app` on a live community session, a
separate live-Mac walkthrough. This spike closes the code/discovery contract
gap that blocked it; a full Tool-Pane app re-run is the natural next step,
matching spike 0057's own note ("Re-run this checklist after #236 lands and
after bridge install").

## Cleanup

- No simulator state changed beyond the taps/swipes/keys above, on the same
  booted iPhone 17 Pro spike 0057's B4 used.
- `baguette` install is a standard, reversible `brew install`; no secrets
  involved.
- Evidence retained under
  `docs/crew/spikes/assets/0058-sim-bridge-install-and-hid-contract-fix/`
  (no secrets).
