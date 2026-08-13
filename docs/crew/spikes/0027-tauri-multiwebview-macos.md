# Spike 0027 — Tauri child webview on macOS (#196)

- **Status:** INCONCLUSIVE (fallback activated)
- **Date:** 2026-08-13
- **Issue:** [#196](https://github.com/Nuncio-hq/crew/issues/196)

## Question

Can a Tauri 2 child webview (`features = ["unstable"]`,
`window.add_child(WebviewBuilder…)`) render a URL at React-owned bounds
inside the main window on macOS — no white flash, smooth resize — so the
Browser tab is an in-pane preview rather than a companion window?

## Decision affected

D-058 / #196 — Browser tab uses a native child webview when the probe
passes; otherwise the huddle-precedent `WebviewWindow` fallback behind
the same TypeScript API.

## Hypothesis

Tauri 2's unstable multiwebview API can parent a child webview to `main`
and `set_bounds` from a ResizeObserver. If that path fails or cannot be
exercised, `WebviewWindowBuilder` (already used by `open_huddle_window`)
is the locked fallback.

## Scope

- Smallest realistic environment: this cloud-agent VM
- In-repo huddle companion (`desktop/src-tauri/src/huddle/window.rs`)
- Tauri 2 crate features already in `desktop/src-tauri/Cargo.toml`

## Exclusions

- Faking a live macOS pass
- Shipping a Browser tab that only works on one OS
- In-browser extensions / profiles

## Pass criteria

Live: child webview draws at bounds inside the real app layout, no white
flash, resize follows the pane.

## Fail criteria

Child webview cannot be parented, flashes, or ignores bounds. Then
activate `WebviewWindow` fallback and keep the child path behind a
runtime probe.

## Inconclusive criteria

The VM is not macOS, so `add_child` cannot be exercised against AppKit.

## Environment

- Commit: Crew `main` at branch start (`cursor/channel-tool-pane-6b23`)
- OS: **Linux** cloud agent (`uname -s` → `Linux`)
- Live macOS multiwebview: **not available**

## Method

1. Confirm OS is not macOS.
2. Read huddle companion constructor (`WebviewWindowBuilder::new`,
   `open_huddle_window`).
3. Confirm Tauri 2 documents `unstable` + `WebviewBuilder` / `add_child`.
4. Do not claim a live pass.

## Results

### Live child webview — INCONCLUSIVE

This VM is Linux. AppKit multiwebview cannot run here. No live pass or
fail was recorded.

### Fallback path — PASS (in-repo)

`open_huddle_window` already creates a labeled `WebviewWindow` on the
same `AppHandle`. That constructor is the chosen fallback for the
Browser tab (URL-anything + Crew-owned dev servers). Same TypeScript
commands (`browser_open` / `set_browser_bounds` / `browser_close`)
select child vs window after a runtime probe.

## Verdict

**INCONCLUSIVE** for the macOS child-webview question. Fallback
activated. Child-webview code is compiled on macOS and selected only
when `probe_child_webview` succeeds; Linux/CI use `WebviewWindow`.

## Follow-up

A macOS machine should re-run this spike against a real window: open the
Browser tab, resize the pane, confirm no white flash. If that pass
lands, flip the probe default — do not change the TypeScript API.
