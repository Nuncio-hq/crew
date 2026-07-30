# NuncioCrew Local Build Plan

- **Status:** complete
- **Approved by:** manager request to create the local build
- **Date:** 2026-07-30

## Outcome

Produce a local, linker-ad-hoc-signed `NuncioCrew.app` that keeps Buzz's release
identity, relay state, and system-Keychain identity while changing only the
visible app name. It has no distribution signing identity or notarization.

## Evidence

- Release builds select Keychain service `buzz-desktop`.
- Debug builds select `buzz-desktop-dev` and cannot satisfy identity reuse.
- This machine has a `buzz-desktop` / `secrets` Keychain entry.
- The installed Buzz app uses identifier `xyz.block.buzz.app`.
- The focused contract was initially RED before the additive flavor files
  existed, then passed `2/2`.

## Phase

1. [Build and verify the local flavor](phase-01-build-and-verify.md)

## Dependencies

- Apple Silicon macOS
- Hermit toolchain
- Existing Keychain entry managed by Buzz

## Scope boundary

- Additive config, plist, build script, contract test, and Crew documentation.
- No Rust edit, updater, distribution signing, notarization, DMG, commit, push,
  key export, or key import.
