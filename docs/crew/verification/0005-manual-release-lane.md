# Verification 0005 — Manual release lane

Date: 2026-07-30

## Scope

Verify the additive NuncioCrew local flavor and manual macOS dev/stable release
lane before the first pull request.

## Automated evidence

- Release contracts: `10/10` passed.
- Full desktop tests: `3857` passed, `1` expected live-relay test skipped,
  zero failed.
- Typecheck, Biome checks, production web build, workflow YAML parse, plist
  validation, and `git diff --check`: passed.
- A real unsigned Tauri distribution spike accepted `0.0.1-dev`, product
  `NuncioCrew`, and bundle identifier `com.nuncio.crew`.
- The rebuilt local artifact reports `NuncioCrew Local`, version `0.5.2`, and
  bundle identifier `xyz.block.buzz.app`; updater configuration is absent.

## Release safety evidence

- Workflow trigger is `workflow_dispatch` only and requires current `main`.
- The protected `nuncio-crew-release` Environment allows branch `main` only
  and requires approval by the configured manager.
- Nine encrypted environment secrets and the updater public variable exist;
  values were not printed or committed.
- The Nuncio updater private key is stored outside the repository with mode
  `0600`; its password is in macOS Keychain.
- A real Tauri signature produced with that private key matched the configured
  public key ID.
- All releases share one concurrency group, reject channel rollback, publish
  immutable assets before advancing a rolling manifest, and preserve pinned
  Cargo dependencies.

## First release proof

- PR #1 merged as `2fe36f3a5190095219bae8c4e029a7aaa37ed895`.
- The final DMG notarization fix merged through PR #2. Release source and both
  release tags point to
  `eb21c3f5b8a172a876b853d53a4aa8af02eefba5`.
- Main CI run
  [`30536991452`](https://github.com/Nuncio-hq/crew/actions/runs/30536991452)
  passed the required `NuncioCrew Gate`.
- Protected signed dry run
  [`30537460233`](https://github.com/Nuncio-hq/crew/actions/runs/30537460233)
  passed without creating a tag or release.
- Protected publish run
  [`30538712572`](https://github.com/Nuncio-hq/crew/actions/runs/30538712572)
  published the
  [`v0.0.1-dev`](https://github.com/Nuncio-hq/crew/releases/tag/v0.0.1-dev)
  prerelease and advanced only
  [`nuncio-crew-dev-latest`](https://github.com/Nuncio-hq/crew/releases/tag/nuncio-crew-dev-latest).
- The public `latest.json` reports `0.0.1-dev`, contains only
  `darwin-aarch64`, points to the immutable versioned updater archive, and its
  signature matches the published `.sig` asset. No stable release exists.
- Public DMG SHA-256:
  `92bc03adf9b4b66134cda9f3e81e580e2de46c41652c4f74a81140a52605952d`.
- The downloaded public DMG passed `hdiutil verify` and stapler validation.
  The app passed strict deep code-sign verification and Gatekeeper assessment
  as `Notarized Developer ID`.
- The app reports bundle identifier `com.nuncio.crew`, version `0.0.1-dev`,
  and all app/sidecar executables are ARM64.
- A launch from the mounted signed DMG reached the identity onboarding screen
  without a Gatekeeper warning or crash.

## Remaining user acceptance

Install the public DMG, select **Use an existing key**, and verify relay
reconnection plus the existing local Project on the manager's real profile.
Updater end-to-end proof remains intentionally pending a second Crew release
whose version is higher than the installed Crew build.
