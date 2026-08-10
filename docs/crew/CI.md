# NuncioCrew CI

## Merge contract

Normal Crew pull requests use one required GitHub check:

```text
NuncioCrew Gate
```

The gate always appears. It requires `CI Policy` and accepts a deliberately
skipped conditional job only when the path classifier says that surface is
unchanged.

A green `NuncioCrew Gate` is not evidence that the Desktop Smoke E2E suite
passed.

This advisory posture is recorded in [`verification/0007`](verification/0007-gate-e2e-shard-relationship.md)
and D-032; revisit making the shards required once #109 and #110 are closed.

| Job | Runs when | Proves |
| --- | --- | --- |
| `CI Policy` | Always | Workflow contract and relevant-path classification |
| `Desktop Fast` | Desktop, Tauri, Rust, or dependency paths change | Desktop lint, tests, and production frontend build |
| `Desktop Rust` | `desktop/src-tauri/**`, `crates/**`, root `Cargo.toml`/`Cargo.lock`, `rust-toolchain.toml`, `Justfile`, or this workflow change | Tauri crate Clippy + unit tests (`cargo test`), including regressions from path deps in `crates/` |
| `buzz-acp` | `crates/buzz-acp/**`, path-dep `crates/buzz-persona/**`, root `Cargo.toml`/`Cargo.lock`, `rust-toolchain.toml`, `Justfile`, or this workflow change | ACP harness lib tests (`cargo test -p buzz-acp --lib`). Not covered by Desktop Rust — `desktop/src-tauri` does not depend on this crate |
| `macOS ARM Package` | Same desktop boundary as Desktop Fast | Unsigned `aarch64-apple-darwin` Tauri package with Nuncio identity |
| `Project Relay` | Project, relay, schema, or Nostr paths change | Kind `30617` local-path lifecycle against an isolated real relay |
| `Desktop Smoke E2E` | Desktop paths change | **nothing that blocks merge** — advisory (`continue-on-error`), excluded from the gate by design (#36/#37) |

The PR package uses placeholder sidecars only to satisfy Tauri's packaging
shape. The manual release workflow builds real sidecars, signs the app,
notarizes it, and verifies the final archive.

## Deliberately excluded from automatic CI

- Buzz web client;
- Flutter mobile;
- Windows and Linux distribution;
- relay and push-gateway container publication;
- Helm and Kubernetes;
- Sprig Linux publication;
- optional mesh-llm native libraries;
- Apple signing, notarization, and updater publication.

These exclusions describe Crew's current macOS Apple Silicon scope. They do
not delete upstream workflows or claim those platforms work.

## Upstream synchronization

`NuncioCrew Upstream Sync` is manual-only. Run it on an upstream-sync branch
after merging `block/buzz` to exercise Rust format, Clippy, unit tests, and
dependency policy for the **root** workspace (and the heavier desktop Tauri
checks such as `desktop-tauri-test-compiled-flags`) without putting those root
checks on every Crew feature PR. Desktop Tauri Clippy + unit tests now also run
automatically on feature PRs via the path-gated `Desktop Rust` job when
`desktop/src-tauri/**`, `crates/**`, root Cargo manifests, the toolchain,
`Justfile`, or this workflow change.
Upstream Sync does not run the inherited integration or cross-platform matrices.

Cut over in this order:

1. verify `NuncioCrew CI` succeeds on the exact merged `main` SHA;
2. require the exact status context `NuncioCrew Gate`;
3. disable inherited `CI` and `Docker image` in GitHub Actions;
4. keep `NuncioCrew CI`, `NuncioCrew Release`, and
   `NuncioCrew Upstream Sync` enabled;
5. verify no disabled inherited job remains required.

Inherited Buzz workflow files remain byte-for-byte unchanged. For rollback,
re-enable the inherited workflows before disabling the Crew gate.

## Release boundary

A green merge gate is not release proof. `NuncioCrew Release` separately
checks the exact current `main` SHA, protected signing inputs, Developer ID,
notarization, entitlements, updater signature, and publication ordering.
