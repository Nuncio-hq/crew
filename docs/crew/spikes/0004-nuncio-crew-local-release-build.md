# Spike 0004 — NuncioCrew local release build

- **Status:** PASS
- **Date:** 2026-07-30

## Question

Can a locally built macOS app be named `NuncioCrew`, run the real bundled
agent sidecars, and reuse the existing Buzz identity without importing or
exposing the private key?

## Decision affected

Whether NuncioCrew can be an additive local build flavor or needs Rust changes
to identity storage.

## Hypothesis

A release build can inherit Buzz's bundle identifier and use the existing
`buzz-desktop` Keychain service while an additive Tauri config and Info.plist
change only the visible app name.

## Scope

- Providers/components: Tauri macOS release bundle, system Keychain, bundled
  Buzz agent sidecars.
- Files or systems: build configuration, generated `.app`, Keychain metadata.
- Time or attempt boundary: one Apple Silicon local release build and launch.

## Exclusions

- Distribution signing, notarization, DMG, or installation into `/Applications`.
- Running Buzz and NuncioCrew concurrently.
- Reading, exporting, rotating, or importing any private key.
- Proving a different machine can reuse this machine's Keychain item.

## Pass criteria

- The bundle is named `NuncioCrew.app`.
- Its display name is `NuncioCrew`.
- It retains identifier `xyz.block.buzz.app`.
- It is compiled in release mode and therefore selects `buzz-desktop`.
- All five bundled sidecars are real executable binaries, not placeholders.
- The app launches without requiring the private key in a command or file.

## Fail criteria

- Rebranding requires a Rust edit.
- The build selects a debug or newly scoped Keychain service.
- Sidecars are missing, empty, or non-executable.
- Reuse requires exposing the private key outside the existing Keychain flow.

## Environment

- Commit: `63496cc1d4c6f1b7c613801bdcc694169dcf391a`
- OS: macOS on Apple Silicon
- Tool/provider versions: recorded by build output
- Authentication class, without secrets: existing macOS system Keychain item

## Method

1. Inspect the release/debug Keychain service selection.
2. Confirm only metadata exists for service `buzz-desktop`, account `secrets`.
3. Write and run an additive build-flavor contract test to RED.
4. Add the smallest config, plist, and reproducible build command.
5. Build sidecars and the locally ad-hoc-signed `.app`.
6. Inspect the generated bundle and launch it without handling secret material.

## Results

- Source inspection: release builds select `buzz-desktop`; debug builds select
  `buzz-desktop-dev`.
- Keychain metadata lookup found service `buzz-desktop`, account `secrets`.
- Installed Buzz uses bundle identifier `xyz.block.buzz.app`.
- The focused contract was RED for the missing flavor files, then GREEN `2/2`.
- The release build produced a 128 MB Apple Silicon `NuncioCrew.app`.
- Bundle inspection found display/name `NuncioCrew` and identifier
  `xyz.block.buzz.app`.
- All five sidecars were non-empty executable Mach-O arm64 binaries.
- Launch smoke loaded the existing Oscar profile, channels, inbox, and relay
  state through the normal Keychain path without entering, exporting,
  displaying, copying, or importing a private key.
- Projects opened at `tauri://localhost/#/projects`.

## Edge cases observed

- A debug build cannot reuse the production identity by design.
- A release build with the same identifier shares app state and single-instance
  scope, so Buzz must be closed before NuncioCrew starts.
- A locally ad-hoc-signed build may cause macOS to ask the user to approve
  Keychain access.

## Limitations

The first launch on another machine or after Keychain ACL changes may still
require the user to approve a macOS Keychain prompt.

## Verdict

PASS: one additive release flavor changed presentation while reusing Buzz
identity and real agent sidecars, with no Rust or secret-handling change.

## Follow-up test contract

The contract must fail until an additive flavor:

- names the product and Info.plist `NuncioCrew`;
- preserves the release identifier;
- performs a real release sidecar build and bundle step;
- never accepts a private key as build input.

## Cleanup

No build or verification command exported, displayed, copied, or imported
secret material. Normal in-app Keychain access remains unchanged. The requested
local `.app` remains under the ignored Tauri target directory. Sidecar bundle
inputs remain in the ignored Tauri binaries directory for future rebuilds.
