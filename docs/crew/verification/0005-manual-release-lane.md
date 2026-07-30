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

## Remaining release proof

No tag or GitHub Release exists yet. After the PR merges, run `v0.0.1-dev` with
`publish=false`, approve the Environment gate, install the resulting signed
artifact on a clean machine, then run the explicit publish workflow only after
that evidence passes.
