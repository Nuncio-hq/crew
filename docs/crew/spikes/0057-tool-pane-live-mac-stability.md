# Spike 0057 — Tool Pane live Mac stability gate (#237)

- **Status:** FAIL (Bar A gate not met)
- **Date:** 2026-08-20
- **Issue:** [#237](https://github.com/Nuncio-hq/crew/issues/237)
- **Related:** [#196](https://github.com/Nuncio-hq/crew/issues/196),
  [#197](https://github.com/Nuncio-hq/crew/issues/197),
  [#234](https://github.com/Nuncio-hq/crew/issues/234),
  [#236](https://github.com/Nuncio-hq/crew/issues/236)
- **Commit under test:** `1790bea696c9dfc8a247d093b1ffd5dd3297f66a`
  (branch `agent/issue-237-tool-pane-live-mac`; installed app
  `/Applications/NuncioCrew.app` was the live binary)

## Question

On a real founder Mac (not a Linux cloud VM), does Tool Pane + agent
desktop control clear **Bar A** once — checklist A–C with no crashes —
so #234 client acceptance can treat live hardware as proven?

## Decision affected

Whether #196 / #197 can be called “done for client acceptance” on the
#234 reality ladder, or whether FAIL rows must become child issues
before all-day / multi-agent claims.

## Hypothesis

Mock E2E + spikes 0027–0034 cover contracts. Live Mac should exercise
child/companion webview, sim bridge (baguette/idb), PTY strip, control
token, and channel switch. Expect partial PASS on agent-control
plumbing; expect FAIL/BLOCKED where setup wall (#236) or missing bridge
blocks instruments.

## Scope

- Live `NuncioCrew.app` on this Mac (`darwin` arm64, macOS 26.5.2)
- Channel `# NuncioCrew project`
  (`9fcd2dc3-5f3e-4f7e-9cb3-3409f33ba7fa`) on
  `wss://lilgroup.communities.buzz.xyz`
- `POST /agent-control` against the running desktop (token from managed
  ACP env; **not recorded**)
- `xcrun simctl` with Xcode at `/Applications/Xcode.app`
- Available sims: iPhone 17 Pro
  (`05DA0D1B-4E34-4678-80C4-D888624313DA`), iPad Pro 13-inch
- Evidence under
  `docs/crew/spikes/assets/0057-tool-pane-live-mac/`

## Exclusions

- Implementing #236 (parallel worktree)
- Installing baguette/idb as production change in this spike
- Bar B (30 min workday) / Bar C pain backlog beyond FAIL child issues
- Multi-hour multi-agent soak
- Claiming PASS for rows not actually exercised

## Pass criteria

Every Bar A row A1–A7, B1–B4, C1–C6 is **PASS** with observable
evidence; no app crash during the run.

## Fail criteria

Any row **FAIL** that blocks day-style use of Browser or Sim, or a
crash. Missing sim bridge and blocked free navigation count as FAIL for
the gate (even when product issues already track them).

## Environment

- OS: macOS 26.5.2 (25F84), Darwin 25.5.0 arm64
- App: `/Applications/NuncioCrew.app` (process `buzz-desktop`)
- Xcode / simctl: `/Applications/Xcode.app/Contents/Developer`
- Agent-control: `http://127.0.0.1:<ephemeral>/agent-control` (live)
- Sim bridges on PATH: **neither** `baguette` nor `idb_companion`
- Auth class: founder desktop session already signed in (no secrets in
  this doc)

## Method

1. Confirm Hermit + `xcrun simctl list devices available`.
2. Attach to live agent-control (Bearer from managed ACP child env).
3. Run C1 `desktop_status`; C2 navigate/snapshot/click; C3
   `sim_snapshot` / `sim_tap`; C4 `lease.take_over` + click under
   `humanHeld`; C5 bad Bearer → `instrument_unreachable`; note C6
   limits.
4. Boot foreign (non-`crew-*`) iPhone 17 Pro via `simctl`; re-check
   governor `booted` stays `0/2` (B4).
5. Capture macOS window shots of Tool Pane Browser (setup wall +
   driving banner).
6. Code-read toolbar Back/Forward/Reload wiring for A5.
7. Do **not** fake PASS for UI steps that need human HID or a second
   stream channel.

## Results

### Checklist summary (Bar A)

| # | Step | Verdict | Evidence |
|---|------|---------|----------|
| A1 | Browser without `tooling.devServer` | **FAIL / BLOCKED-on-236** | Live pane showed workspace/setup wall (“No workspace… / Pick a folder…”); free URL entry not proven. See #236. Shot: `assets/0057-tool-pane-live-mac/02-browser-setup-wall-agent-driving.png` (also posted on the PR) |
| A2 | External HTTPS + local URL | **FAIL** | `browser_navigate` to `about:blank` / `https://example.com` timed out (~20–60s); no stable page load |
| A3 | Resize / viewport preset | **INCONCLUSIVE** | Viewport select exists in UI; live resize within 1s not measured (HID / Accessibility blocked for automation) |
| A4 | Start Crew dev server from strip | **INCONCLUSIVE** | No bound workspace / no canvas `tooling.devServer` on this channel; strip Start path not exercised |
| A5 | Back / forward / reload | **FAIL** | Toolbar icons render (`browser-back` / `forward` / `reload`) but `ToolbarIcon` has **no `onClick`** for those three in `BrowserTab.tsx` |
| A6 | Hide pane 5s → reopen | **INCONCLUSIVE** | Pane was observed closed later; URL/server restore after timed hide not measured |
| A7 | Switch channel → return | **INCONCLUSIVE** | Community localStorage lists one stream channel (`NuncioCrew project`) + DMs only — no second stream to leak-test |
| B1 | Boot sim from Sim tab; mirror ~15s | **FAIL** | Agent `sim_snapshot` → `bridge_missing` (“Simulator bridge is not installed”; install hint `brew install baguette`) |
| B2 | Tap / swipe / type | **FAIL** | Blocked on B1 / missing bridge |
| B3 | Hide pane 5s (stream pause) | **INCONCLUSIVE** | Blocked on B1 |
| B4 | Foreign `simctl` device | **PASS** | Booted stock iPhone 17 Pro via `simctl`; `desktop_status` stayed `governor.booted: "0/2"`, `sim_state: null` — governor did not claim/shutdown foreign device |
| C1 | `desktop_status` | **PASS** | Returns `browser_url`, `sim_state`, `governor.booted`, `lease` (`json/c1-desktop_status.json`) |
| C2 | `browser_snapshot` → `browser_click` | **FAIL** | After navigate attempt: `timeout: browser bridge did not reply`; no stable ref/digest |
| C3 | `sim_snapshot` → `sim_tap` | **FAIL** | `bridge_missing` (`json/c3-sim_snapshot.json`) |
| C4 | Human Take over mid-turn | **PASS** | UI banner “Hermes is driving” observed; `lease.take_over` → `humanHeld`; subsequent `browser_click` → `lease_held` (`json/c4-click-while-human.json`) |
| C5 | Restart desktop → stale token | **PASS** (token path) | Bad Bearer → `instrument_unreachable` with message “missing or stale desktop control token…” (`json/c5-stale-token.json`). Full app restart + ACP respawn **not** performed |
| C6 | Agent drives pane closed | **INCONCLUSIVE** | Instrument≠pane is designed (D-059); live mid-flight sidebar-dot + open reveal not fully driven this session |

### Crashes

None observed during the run.

### Spikes 0027–0034 context

Prior spikes remain largely **INCONCLUSIVE on hardware** (Linux VM).
This run is the first founder-Mac Bar A attempt recorded in-repo for
#237. It does **not** upgrade 0027/0028/0031 live verdicts to PASS.

## Edge cases observed

1. **Setup wall vs Custom URL.** Source at this commit has
   `showSetup = !tooling?.devServer && subject !== "custom"`, but the
   installed UI still presented a workspace/setup wall that blocked a
   clean A1 pass — tracked by #236.
2. **Agent lease while wall visible.** Driving banner and
   `agentHeld` lease appeared even when the preview showed the setup
   wall — control plane ahead of a usable surface.
3. **Navigate hang.** `browser_navigate` did not return within 20–60s
   (blank or HTTPS); likely stuck in ensure/open/bridge path rather
   than a clean `origin_blocked`.
4. **Foreign sim.** Stock `simctl` boot left governor caps untouched
   (`0/2`) — matches D-058 identity split.

## Limitations

- No Accessibility permission for `osascript` System Events — limited
  HID automation of the native window.
- Only one stream channel in this community — A7 incomplete.
- Installed `.app` may lag or lead git `1790bea69`; evidence is from
  the running binary + this tree’s source read for A5.
- Bar B not attempted.
- Spike number **0057** (0056 reserved on parallel Hermes branch).

## Verdict

**FAIL** — Bar A is not met on this founder Mac. Agent-control
plumbing (C1, C4, C5 token error, B4 foreign isolation) works;
Browser live drive (A2/C2) and Sim mirror (B1–B3/C3) fail; A1 is
blocked on #236; A5 back/forward/reload are unwired.

## Follow-up test contract

Child issues from FAIL rows:

1. [#246](https://github.com/Nuncio-hq/crew/issues/246) — missing sim bridge (`baguette` / `idb_companion`) — B1/B2/C3
2. [#247](https://github.com/Nuncio-hq/crew/issues/247) — live `browser_navigate` / bridge reply hang — A2/C2
3. [#248](https://github.com/Nuncio-hq/crew/issues/248) — wire Browser toolbar back / forward / reload — A5

A1 remains on [#236](https://github.com/Nuncio-hq/crew/issues/236) (not duplicated).

Re-run this checklist after #236 lands and after bridge install; require
RED→GREEN only where product code changes.

Spike 0058 fixes and CLI-verifies the sim-bridge HID contract for #246 (B1
`bridge_missing` + the `--width`/`--height`/subcommand bugs B2 would have
hit next); the through-app re-run of B1–B3/C3 is still open.

## Cleanup

- Foreign iPhone 17 Pro left **Shutdown** after B4.
- No production code changed in this spike.
- Evidence retained under
  `docs/crew/spikes/assets/0057-tool-pane-live-mac/` (no secrets).
- Local proof session under `.ade/proof/` (not committed).
