# Phase 03 — First signed release and verification

## Context links

- [Plan](plan.md)
- [`docs/crew/RELEASING.md`](../../docs/crew/RELEASING.md)
- [`docs/crew/verification/0005-manual-release-lane.md`](../../docs/crew/verification/0005-manual-release-lane.md)
- [`nuncio-crew-release.yml`](../../.github/workflows/nuncio-crew-release.yml)

## Overview

Priority: release blocking. Status: pending merge and protected runner proof.

Publish `v0.0.1-dev` from the verified `main` SHA using the existing manual
release workflow. Never expose or copy App Store API credential contents.

## Preflight

1. Record the exact merged `main` commit and confirm the tag and release do not
   exist.
2. Confirm the `nuncio-crew-release` Environment allows only `main`, requires
   approval, and has all documented encrypted Apple/updater secrets plus the
   updater public-key variable.
3. Confirm committed Buzz manifests and source metadata remain pinned to
   `0.5.2`; only the disposable CI checkout is patched to `0.0.1-dev`.
4. Confirm the dev rolling manifest is distinct from stable and that stable
   clients cannot receive dev releases.

## Dry run

Dispatch `NuncioCrew Release` with:

```text
version: v0.0.1-dev
channel: dev
ref: <exact merged main SHA>
publish: false
```

Approve the protected Environment, then verify:

- immutable source, version/channel, and absent-tag checks pass before signing;
- Developer ID identity and `com.nuncio.crew` metadata are correct;
- `codesign --verify`, Gatekeeper assessment, notarization, stapling,
  entitlements, and updater-key identity checks pass;
- the downloaded artifact launches on a clean Apple Silicon Mac and reports
  `v0.0.1-dev` on the dev channel;
- no tag, GitHub Release, or rolling manifest was created.

## Publish and verify

1. Rerun with identical version, channel, and source SHA; change only
   `publish: true`.
2. Confirm the immutable `v0.0.1-dev` tag and versioned GitHub Release.
3. Verify every expected app/update asset is present, downloadable, and signed.
4. Verify the dev `latest.json` points to `v0.0.1-dev`, contains the updater
   signature, and the stable manifest is unchanged.
5. Repeat clean-install launch, Gatekeeper, bundle ID, version, channel, relay
   connection, and local-project smoke checks from the published asset.
6. Record the workflow run, release URL, asset hashes, signature/notarization
   evidence, and manifest URL in the verification document.

## Update limitation

`v0.0.1-dev` is lower than Buzz/local `0.5.2`, so the first NuncioCrew install
is manual. A real updater E2E needs a second, higher signed Crew version; do not
claim update-path proof from the first release alone.

## Rollback

- Before publication: stop; repair secrets outside Git; rerun dry mode.
- Draft failure: delete or retain the draft for diagnosis; no manifest moves.
- Bad release before manifest update: leave it out of the rolling channel and
  publish a higher fixed version.
- Bad dev manifest: restore the last known-good `latest.json`; never mutate or
  reuse the version tag.
- Lost updater private key: recover the original key; do not silently rotate it.

## Success criteria

The signed public asset installs cleanly, Apple checks pass, the dev manifest
selects only `v0.0.1-dev`, stable is untouched, and evidence contains no secret.

## Unresolved questions

Updater E2E remains pending a second signed Crew version.
