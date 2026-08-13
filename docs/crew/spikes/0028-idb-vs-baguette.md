# Spike 0028 — idb_companion vs baguette for headless sim mirror (#196)

- **Status:** PASS (labeled in-repo / public-docs evidence; not live)
- **Date:** 2026-08-13
- **Issue:** [#196](https://github.com/Nuncio-hq/crew/issues/196)

## Question

Which external binary should the sim-bridge discovery ladder prefer for
headless MJPEG + HID (tap/swipe/scroll/key) on current iOS: Facebook
`idb_companion` or `baguette`?

## Decision affected

D-058 / #196 — sim-bridge is discovered like `gh` (`available` /
`missing` / `failed` + install card). The ladder must accept either
binary; the preferred first hit is a product choice.

## Hypothesis

`baguette` is the better default on current iOS (Indigo HID, MJPEG/avcc,
`describe-ui`, single binary). `idb_companion` remains the brew-mature
fallback.

## Scope

- This Linux VM (no `simctl`, no CoreSimulator)
- In-repo: `gh_cli.rs` availability ladder, `just mobile-dev` Simulator.app
  usage, zero product `simctl` before this issue
- Public docs / issue text for baguette vs idb

## Exclusions

- Live CPU-at-20fps / input-latency measurement (no simulator here)
- Android emulator
- Treating Simulator.app as the mirror

## Pass criteria

A preferred binary is named with cited evidence, the other stays on the
ladder, and CI never requires a real simulator.

## Fail criteria

Picking a binary that cannot do headless MJPEG+HID, or requiring
Simulator.app for the user-visible mirror.

## Environment

- OS: Linux cloud agent — **no simctl / CoreSimulator**
- Live HID/MJPEG: **not available**
- Evidence class: in-repo + public docs, labeled as such

## Method

1. Confirm this checkout has no `simctl` wrappers in product Rust/TS.
2. Confirm `gh_cli.rs` is the discovery-ladder precedent.
3. Compare published capabilities:
   - **idb_companion** (facebook/idb): mature Homebrew tap, companion
     process, screenshot/video + HID; historically tied to a companion
     + client pair; video is often ffmpeg/AVFoundation rather than a
     trivial MJPEG multipart URL.
   - **baguette** (mobile-dev tooling around current CoreSimulator):
     single binary, advertised MJPEG/avcc stream, Indigo HID conventions
     on iOS 26, `describe-ui`. Issue #196 names these as the reason it
     is in contention.

## Results

### Live stream + HID — INCONCLUSIVE (no hardware)

Cannot measure 20 fps CPU or tap latency on this VM.

### In-repo — PASS

- Product Rust/TS: no `simctl` before this work. Simulator is
  script-only (`just mobile-dev` opens Simulator.app).
- Discovery precedent: `desktop/src-tauri/src/commands/gh_cli.rs`
  (`available` / `CliMissing`, Homebrew PATH, no cache of misses).

### Public / issue evidence — PASS (labeled)

Issue #196: baguette is “single binary, correct iOS-26 Indigo HID
conventions, MJPEG/avcc stream + `describe-ui`”; idb is “mature, brew”.
That matches the founder-locked pipeline (MJPEG multipart into the pane,
HID back into a **headless** device).

## Pick

**Prefer `baguette`, then `idb_companion`.** Discovery tries both.
Missing → install card with copyable brew commands for each + Recheck.
Failed spawn → `failed` with the stderr snippet.

Install hints (copyable):

```text
brew install baguette
brew tap facebook/fb && brew install idb-companion
```

## Verdict

**PASS** as a labeled docs/in-repo pick. Re-measure on a Mac with a
booted `crew-*` device before treating fps/latency numbers as fact.

## Follow-up

On macOS: boot a headless iPhone simulator, stream 20 fps, tap once,
record CPU and round-trip. Switch the ladder order only if baguette
cannot HID on the current runtime.
