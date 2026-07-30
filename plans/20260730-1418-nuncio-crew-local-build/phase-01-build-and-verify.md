# Phase 01 — Build and verify

## Context links

- [`plan.md`](plan.md)
- [`../../docs/crew/spikes/0004-nuncio-crew-local-release-build.md`](../../docs/crew/spikes/0004-nuncio-crew-local-release-build.md)

## Overview

- **Priority:** current
- **Status:** complete
- **Description:** add and validate one local macOS build flavor.

## Key insights

- Key reuse depends on release compilation and the `buzz-desktop` service, not
  the visible product name.
- Keeping `xyz.block.buzz.app` also reuses relay and app state.
- Real sidecars must be built and copied before Tauri bundles the app.

## Requirements

- Generated bundle is `NuncioCrew.app`.
- Display and bundle name are `NuncioCrew`.
- Identifier remains `xyz.block.buzz.app`.
- All five sidecars are non-empty executables.
- The build accepts no private-key input.

## Architecture

The additive config overrides presentation only. The release binary continues
to use Buzz's existing identifier and Keychain boundary.

## Related code files

Create:

- `desktop/src-tauri/tauri.nuncio-crew.conf.json`
- `desktop/src-tauri/Info.NuncioCrew.plist`
- `scripts/build-nuncio-crew-local.sh`

Modify:

- Crew spike, state, testing, and verification documentation after evidence.

Delete: none.

## Implementation steps

1. Add the Tauri override and NuncioCrew Info.plist.
2. Add a release-sidecar build script with no secret input.
3. Run the focused contract to GREEN.
4. Build the local app and inspect bundle metadata and sidecars.
5. Launch the exact artifact and record user-visible identity behavior.
6. Run affected checks and independent review.

## Todo list

- [x] Record spike evidence.
- [x] Capture intended RED.
- [x] Add flavor files.
- [x] Build and inspect artifact.
- [x] Launch smoke.
- [x] Finish regression and docs.

## Success criteria

- Focused and affected desktop tests pass.
- `NuncioCrew.app` exists and its Info.plist matches the contract.
- The bundle contains five executable non-empty sidecars.
- No secret appears in source, commands, or logs.

## Risk assessment

- Same identifier means Buzz and NuncioCrew must not run concurrently.
- macOS may request Keychain approval for the locally ad-hoc-signed executable.
- Output without a distribution identity or notarization is not suitable for
  distribution.

## Security considerations

The script accepts no key input, sanitizes known secret-bearing environment
variables before subprocesses, and never exports or displays a private key.
Normal app identity access remains inside the existing system-Keychain path.

## Next steps

Test the installed providers and Project workspace flow after the app launches.
