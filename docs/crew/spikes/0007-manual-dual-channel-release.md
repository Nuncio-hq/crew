# Spike 0007 — Manual dual-channel NuncioCrew release

## Question

Can Crew add a manual macOS dev/stable release lane while leaving Buzz's
workflow and committed version untouched?

## Pass criteria

- Crew can use an additive workflow with only `workflow_dispatch`.
- Tauri accepts the Nuncio config stack and `0.0.1-dev`.
- Local and distributed identities can be configured independently.
- Updater endpoints and rolling manifests can be separated by channel.
- Existing Apple material is sufficient in type for Developer ID signing and
  API-key notarization.
- Updater signing remains an independent Nuncio key.

## Evidence

- Buzz release jobs are guarded to `block/buzz` and contain Block-only signing
  and release URLs, so an additive Crew workflow is required.
- Tauri enables the updater only when both updater public key and endpoint are
  present.
- Tauri CLI confirmed multiple `--config` files merge in argument order.
- A real unsigned Apple Silicon build with version `0.0.1-dev`, product
  `NuncioCrew`, and identifier `com.nuncio.crew` completed successfully.
- The generated app Info.plist contained the intended version and identifier.
- The local AppStoreAPI directory contains an App Store Connect key and a valid
  Developer ID Application certificate; secret contents were not logged or
  copied into the repository.
- Tauri requires a separate updater keypair and does not allow unsigned
  updates.
- RED run: eight contracts, one existing pass and seven intended failures.
- GREEN run after implementation: ten contracts passed.
- Red-team review found and the implementation closed four production risks:
  branch access to signing secrets, updater pointers to draft assets, mismatched
  updater keys, and concurrent channel rollback.
- Release CI now accepts only current `main`, uses a protected environment,
  serializes all versions, and refuses equal or lower rolling versions.
- The artifact-version patch updates only the root `buzz-desktop` lockfile
  entry; dependency resolution remains pinned.

## Limits

- The protected GitHub Environment and encrypted credentials are configured,
  but no workflow has run because its implementation is not merged to `main`.
- No signed/notarized build or public release was created.
- Updater installation cannot be proven until two signed Crew versions exist.
- First install from the `0.5.2` local flavor to `0.0.1-dev` is manual.
- Distributed identity does not yet provide automatic Buzz Keychain migration.

## Verdict

**PASS**

The design is feasible as an additive, manual, macOS-first release lane.
Publication remains gated on encrypted credential setup, PR merge, a signed dry
run, and explicit manager execution.
