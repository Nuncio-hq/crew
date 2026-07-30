# Verification 0002 — NuncioCrew local build

- **Date:** 2026-07-30
- **Result:** PASS
- **Scope:** locally ad-hoc-signed Apple Silicon release and identity reuse

## Manager-visible outcome

The repository produces `NuncioCrew.app`. It opens with the existing Buzz
profile, relay state, channels, and inbox without importing or exposing the
private key.

## Build evidence

Command:

```text
./scripts/build-nuncio-crew-local.sh
```

Result:

- release sidecars built successfully;
- frontend production build passed;
- native release build passed with two existing dead-code warnings;
- Tauri produced one local `NuncioCrew.app` with only linker ad-hoc signing;
- artifact size: 128 MB.

Bundle inspection confirmed:

- `CFBundleDisplayName = NuncioCrew`;
- `CFBundleName = NuncioCrew`;
- `CFBundleIdentifier = xyz.block.buzz.app`;
- main executable and all five sidecars are Mach-O arm64;
- all sidecars are non-empty and executable.

## Identity and launch evidence

A metadata-only Keychain query found the existing `buzz-desktop` service and
`secrets` account. No secret value was requested.

Computer Use launched the initial generated artifact. The app loaded profile
`Oscar`, existing channels, inbox history, and relay state. Projects opened at
the expected internal route. No key entry or Keychain prompt was needed on this
launch.

Review then hardened only the build environment and target selection. The
target-qualified final artifact was rebuilt and its metadata and binaries were
independently inspected. It was not relaunched automatically because the Mac
became locked after physical user input; the runbook begins with that manual
launch step.

## Test evidence

- Focused build-flavor contract: `2/2` passed.
- Full desktop suite: `3826` passed, one gated live-relay test skipped, zero
  failed.
- TypeScript typecheck: passed.
- Biome, file-size, text-unit, and pubkey-truncation checks: passed.
- Info.plist lint, shell syntax, and Git whitespace checks: passed.

Biome reported two existing informational template-literal suggestions in
`personaCatalogRelay.test.mjs`; they do not fail the check.

## Accepted limitations

- The app has no distribution signing identity and is not notarized.
- Buzz and NuncioCrew share identity, app data, deep links, and single-instance
  scope; do not run them concurrently.
- A future launch may require the user to approve Keychain access.
- This verification did not create a new user Project on the relay.
- Provider filesystem behavior remains covered by Project workspace spike 0001.

## Cleanup

No build or verification command exported, displayed, copied, imported, or
wrote a secret. Normal in-app Keychain access remains unchanged. The generated
app and sidecar bundle inputs remain in ignored build directories. No commit or
push was made.
