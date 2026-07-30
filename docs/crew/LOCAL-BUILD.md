# NuncioCrew Local Build

## What this build is

`NuncioCrew Local.app` is a local Apple Silicon release build of Crew. It has only
linker ad-hoc signing, with no distribution signing identity or notarization.
It changes the visible app name while preserving Buzz's release identifier:

```text
product name: NuncioCrew Local
bundle identifier: xyz.block.buzz.app
Keychain service: buzz-desktop
```

This is deliberate. The app reuses the existing Buzz identity, relay
configuration, channels, and app state through Buzz's normal Keychain path. No
user or build command exports, displays, copies, or imports an `nsec`.

Do not run Buzz and NuncioCrew at the same time. They share single-instance
scope, app data, deep links, and Keychain storage.

## Build

From the repository root:

```text
./scripts/build-nuncio-crew-local.sh
```

The script:

1. activates the repository Hermit toolchain;
2. builds the five agent sidecars in release mode;
3. verifies and copies the real sidecars into the Tauri bundle input;
4. builds a locally ad-hoc-signed release `.app` with the additive NuncioCrew
   config and no distribution identity.

It accepts no key input and removes known identity, provider API, Cargo target,
and Rust flag overrides from its build environment.

Artifact:

```text
desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/NuncioCrew Local.app
```

## First launch

1. Quit Buzz completely.
2. Open `NuncioCrew Local.app` from the artifact path.
3. If macOS asks whether NuncioCrew may access the existing Keychain item,
   inspect the app name and choose `Allow` or `Always Allow` yourself.
4. Confirm the profile name/avatar and existing channels appear.
5. Confirm the expected relay/community is selected.

Do not paste an `nsec` into a terminal, build command, source file, test, or
issue. A key previously shared through chat should be treated as exposed and
rotated before long-term use.

## Test Project local workspace

The relay is authoritative, but no separate CLI registration is required:

1. Open **Projects** in NuncioCrew.
2. Click the round **+** button and choose **Repository**.
3. Select an existing local folder. Cancel once first and confirm no Project
   or channel appears.
4. Repeat, select the folder, review the exact plaintext path and relay
   destination, and adjust the default Project name if needed.
5. Choose **Add Project**.
6. Confirm the Project appears only after publication completes.
7. Quit and reopen NuncioCrew.
8. Return to **Projects** and confirm the same Project is reconstructed from
   the relay.
9. Mention an agent in the Project's bound channel and ask it to inspect a
   harmless file by absolute path.

Expected result:

- Project identity remains `(pubkey, identifier)`;
- the local path is location metadata on kind `30617`;
- no Git remote is inspected and no `clone` tag is fabricated in this slice;
- the selected folder is not cloned, initialized, or modified;
- `session/new.cwd` remains unchanged;
- the agent receives the path through Project-channel context.

## Rebuild checks

```text
. ./bin/activate-hermit
bash -n scripts/build-nuncio-crew-local.sh
plutil -lint desktop/src-tauri/Info.NuncioCrew.plist
cd desktop
node --import ./test-loader.mjs --experimental-strip-types \
  --test src/testing/nuncio-crew-local-build-contract.test.mjs
pnpm test
pnpm typecheck
pnpm check
```

The local build shows `v<upstream version> · Local` in Settings. It has no
distribution signing identity, updater endpoint, updater public key, updater
artifact, notarization, or DMG. Rebuild it after pulling upstream changes that
affect the desktop app or Rust sidecars.
