# Spike 0031 — WKWebView JS-bridge snapshot quality + ref stability (#197)

- **Status:** INCONCLUSIVE (live WKWebView) / PASS (in-repo contract)
- **Date:** 2026-08-13
- **Issue:** [#197](https://github.com/Nuncio-hq/crew/issues/197)

## Question

Can an injected JS bridge on the #196 child webview / `WebviewWindow`
fallback mint an a11y-style tree with stable `e1..eN` refs and a
`snapshot_digest` that survives a no-op re-snapshot?

## Decision affected

D-059 / #197 — snapshot-with-stable-refs is the interaction model.
Execution stays on Crew's webview, not Playwright MCP.

## Hypothesis

A same-document traversal (role + accessible name + tree path) assigned
in document order yields identical refs and digest when the DOM is
unchanged. Digest changes when a node is added or removed.

## Scope

- In-repo fake DOM + the production JS bootstrap (`browser_bridge.js`)
- This cloud-agent VM (Linux; no AppKit WKWebView)

## Exclusions

- Faking a live macOS WKWebView pass
- Adopting Playwright MCP as a runtime

## Pass criteria

1. Two snapshots of an unchanged tree return the same refs and digest.
2. Adding a node changes the digest; actions carrying the old digest
   fail `stale_ref`.

## Fail criteria

Refs scramble on a no-op re-snapshot of the same tree.

## Inconclusive criteria

The VM is not macOS, so live WKWebView injection cannot run here.

## Environment

- OS: **Linux** cloud agent (`uname -s` → `Linux`)
- Live WKWebView: **not available**

## Method

1. Confirm OS is not macOS.
2. Implement the ref/digest contract against a fake DOM (same algorithm
   the injected script uses).
3. Do not claim a live WKWebView pass.

## Results

### Live WKWebView — INCONCLUSIVE

This VM is Linux. AppKit / WKWebView cannot run here.

### In-repo contract — PASS

`agent_control` snapshot tests pin identical refs across two snapshots of
the same tree and `stale_ref` after mutation.

## Verdict

INCONCLUSIVE for live WKWebView (environment). PASS for the in-repo
algorithm. Production injects the same bootstrap via `WebviewWindow.eval`
(Linux/CI fallback) or the macOS child webview.

## Follow-up test contract

`snapshot_refs_stable_across_unchanged_tree` and
`mutating_the_tree_invalidates_digest`.

## Cleanup

No disposable live webviews were created.
