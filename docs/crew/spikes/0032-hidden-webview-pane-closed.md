# Spike 0032 — Hidden-webview execution while the pane is closed (#197)

- **Status:** INCONCLUSIVE (live) / PASS (in-repo contract)
- **Date:** 2026-08-13
- **Issue:** [#197](https://github.com/Nuncio-hq/crew/issues/197)

## Question

Can the browser instrument evaluate / click while the Tool Pane is
closed (instrument ≠ pane), using a 1×1 px fallback if the OS suspends
hidden webviews?

## Decision affected

D-059 locked: instruments live in the Governor and work with the pane
closed. Sidebar dots show activity.

## Hypothesis

#196 already tracks `WebviewHolding.hidden`. Agent tools call
ensure-on-use, attach a hidden webview (or 1×1 window fallback), and
drive it without opening the pane.

## Scope

- Governor hidden-webview records + control-runtime ensure path
- Linux VM: no live AppKit child webview

## Exclusions

- Claiming a live macOS hidden-webview pass

## Pass criteria

A control `browser_evaluate` against a hidden holding succeeds in the
fake host; status still shows the webview as hidden / agent-active.

## Inconclusive criteria

Live WKWebView suspension behavior cannot be measured on Linux.

## Environment

- OS: Linux cloud agent
- Fallback: `WebviewWindow` at 1×1 px (huddle-precedent constructor)

## Results

Live hidden-webview suspension: INCONCLUSIVE.
In-repo: agent tools do not require `pane_visible`; FakeBrowser serves
clicks while `hidden: true`. Production `LiveBrowser` opens the labeled
window at 1×1 when the pane is closed.

## Verdict

INCONCLUSIVE live / PASS in-repo. 1×1 fallback is the locked Linux/CI
path (same as spike 0027).

## Follow-up test contract

`browser_click_succeeds_while_pane_closed`.

## Cleanup

None.
