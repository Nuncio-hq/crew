# NuncioCrew Release Runbook

## Release model

NuncioCrew publication is always a manager action. Pushes, pull requests,
merges, tags, schedules, and upstream Buzz releases do not publish anything.

The GitHub workflow builds one signed and notarized macOS Apple Silicon
artifact. Its default is a signed dry run; publication requires explicitly
enabling the `publish` input.

## Identities

| Build | Product | Bundle identifier | Updater |
| --- | --- | --- | --- |
| Local | `NuncioCrew Local` | `xyz.block.buzz.app` | Disabled |
| Dev | `NuncioCrew` | `com.nuncio.crew` | Dev manifest |
| Stable | `NuncioCrew` | `com.nuncio.crew` | Stable manifest |

The committed desktop manifests remain at the pinned Buzz version. Release CI
patches its temporary checkout to the Crew version and never commits that
change.

## Versions, release tags, and channels

The workflow input is a semantic Crew version. The release helper derives a
Crew-owned immutable GitHub tag while keeping the packaged app and updater
manifest on the semantic version:

```text
channel  workflow input   immutable GitHub tag     artifact/manifest version
dev      v0.0.5-dev       crew-v0.0.5-dev          0.0.5-dev
dev      v0.0.5-dev.1     crew-v0.0.5-dev.1        0.0.5-dev.1
stable   v0.0.5           crew-v0.0.5              0.0.5
```

An inherited or historical Buzz tag such as `v0.0.5` does not collide with the
Crew release because collision checks and publication use `crew-v0.0.5`.

Dev publication updates `nuncio-crew-dev-latest`. Stable publication updates
both `nuncio-crew-stable-latest` and `nuncio-crew-dev-latest`. This lets dev
users graduate to stable while preventing stable users from receiving dev.

Every installed build contains exactly one updater endpoint. Rolling releases
hold only `latest.json`; updater archives live on immutable versioned releases.

## GitHub configuration

Create a GitHub Environment named `nuncio-crew-release`, allow deployments
only from branch `main`, and store these encrypted environment secrets there:

```text
NUNCIO_APPLE_CERTIFICATE_BASE64
NUNCIO_APPLE_CERTIFICATE_PASSWORD
NUNCIO_APPLE_SIGNING_IDENTITY
NUNCIO_APPLE_TEAM_ID
NUNCIO_APPLE_API_ISSUER
NUNCIO_APPLE_API_KEY_ID
NUNCIO_APPLE_API_PRIVATE_KEY
NUNCIO_TAURI_SIGNING_PRIVATE_KEY
NUNCIO_TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

Required repository variable:

```text
NUNCIO_TAURI_UPDATER_PUBLIC_KEY
```

The Apple certificate signs the app. The App Store Connect API key authenticates
notarization. The Tauri key signs updater artifacts. They are not interchangeable.
The protected environment prevents a workflow copied to another branch from
receiving production signing material.

Never commit, print, attach, or copy private keys into Actions variables. Key
files and passwords remain outside the repository and enter Actions only
through encrypted secrets.

## Before running

1. Merge the release implementation PR to `main`; releases accept only the
   current `origin/main` HEAD.
2. Confirm the required `NuncioCrew Gate` on `main` is green.
3. Copy the exact 40-character commit SHA from `main`.
4. Derive `crew-<workflow-version>` and confirm that namespaced Git tag and
   GitHub release do not already exist. An unprefixed inherited tag is
   irrelevant.
5. For updater E2E between distributed Crew builds, confirm the new version is
   greater than the installed Crew version. For `v0.0.5`, preserve a working
   `0.0.4` installation as the update source. Exempt the manual first
   `v0.0.1-dev` install described below; it cannot update from Buzz `0.5.3`.
6. Run the focused release contract from [`TESTING.md`](TESTING.md).
7. Dispatch from branch `main`, then approve the
   `nuncio-crew-release` Environment gate when GitHub requests it.

## Signed dry run

Open **Actions → NuncioCrew Release → Run workflow** and enter:

```text
version  v0.0.1-dev
channel  dev
ref      <exact main commit SHA>
publish  false
```

The job waits for approval from the configured release reviewer before GitHub
exposes signing secrets.

The workflow must:

1. prove the SHA is the current `origin/main` HEAD;
2. validate the semantic version and channel, deriving the namespaced release
   tag;
3. reject an existing Crew-prefixed tag;
4. import signing material into a disposable Keychain;
5. patch only the CI checkout's artifact version;
6. build, sign, notarize, and staple;
7. verify codesign, Gatekeeper, entitlements, updater archive, and that its
   signature key ID matches the public key embedded in the app;
8. upload a private workflow artifact;
9. delete the disposable Keychain;
10. create no tag or GitHub Release.

Download the workflow artifact and perform the clean-install checks before
publishing.

## Publish

Run the same workflow again with identical version, channel, and SHA, changing
only:

```text
publish  true
```

Publication creates the Crew-prefixed version tag and versioned release only
after the signed build passes. A dev version becomes a GitHub prerelease. The
workflow uploads the DMG, updater archive, and signature, then updates the
selected rolling `latest.json`. The manifest keeps the semantic version and
human-facing note (`NuncioCrew v0.0.5`) while its archive URL points to the
immutable namespaced release (`crew-v0.0.5`). All release runs share one queue,
and an existing channel refuses an equal or lower version. The immutable
versioned release becomes public before the rolling manifest changes, so a
failed channel update leaves clients on the previous known-good version.

## End-to-end verification

CI success and a visible GitHub release are not sufficient proof.

Verify:

- `codesign --verify --deep --strict` passes;
- Gatekeeper accepts the application;
- the DMG has a valid stapled notarization ticket;
- a clean machine installs and launches the app;
- Settings reports the expected Crew version and channel;
- the expected relay and identity can be configured;
- an older signed dev build detects, downloads, verifies, installs, and
  relaunches into the new version;
- for stable `0.0.5`, an installed stable `0.0.4` detects, downloads, verifies,
  installs, and relaunches into `0.0.5` without replacing its Crew application
  data;
- stable does not see a dev-only manifest;
- dev sees a later stable manifest.

Record the commands, source SHA, workflow run, release URL, installed versions,
application-data check, and observed relaunch in a new file under
`verification/`. Do not mark the release complete from CI or manifest evidence
alone; the installed updater relaunch is a post-merge publication gate.

## First release limitation

`NuncioCrew Local` reports the pinned Buzz version `0.5.3`. Tauri correctly
refuses to update it to lower `0.0.1-dev`. Install the first distributed dev
build manually. Auto-update evidence begins with a signed lower Crew dev probe
or the next monotonically higher Crew release.

## Failure and recovery

- Validation failure: change the input; do not weaken the guard.
- Signing/notarization failure: keep `publish=false`, inspect Apple output, and
  rotate or repair credentials outside the repository.
- Partial draft release: leave it draft while diagnosing; do not point a
  rolling manifest at an incomplete release.
- Bad updater manifest: restore the last known-good `latest.json` on the rolling
  release. Never reuse an immutable Crew-prefixed version tag for different
  bits.
- Published version but unchanged rolling manifest: keep the immutable release,
  diagnose the channel step, then advance the rolling manifest only after its
  version and signature are verified.
- Lost Tauri private key: existing installs cannot trust a replacement key.
  Recover the original key or ship a key rotation signed by the old key.
