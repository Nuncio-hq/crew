# NuncioCrew CI

## Merge contract

Normal Crew pull requests use one required GitHub check:

```text
NuncioCrew Gate
```

The gate always appears. It requires `CI Policy` and accepts a deliberately
skipped conditional job only when the path classifier says that surface is
unchanged.

A green `NuncioCrew Gate` is not evidence that the Desktop Smoke E2E suite or
the Desktop E2E Integration suite passed.

Smoke advisory posture is recorded in
[`verification/0007`](verification/0007-gate-e2e-shard-relationship.md) and
D-032. Integration advisory posture is recorded in D-047 (#147); revisit making
either lane required only by an explicit founder decision.

| Job | Runs when | Proves |
| --- | --- | --- |
| `CI Policy` | Always | Workflow contract and relevant-path classification |
| `Desktop Fast` | Desktop, Tauri, Rust, or dependency paths change | Desktop lint, tests, and production frontend build |
| `Desktop Rust` | `desktop/src-tauri/**`, `crates/**`, root `Cargo.toml`/`Cargo.lock`, `rust-toolchain.toml`, `Justfile`, or this workflow change | Tauri crate Clippy + unit tests (`cargo test`), including regressions from path deps in `crates/` |
| `buzz-acp` | `crates/buzz-acp/**`, path-dep `crates/buzz-persona/**`, root `Cargo.toml`/`Cargo.lock`, `rust-toolchain.toml`, `Justfile`, or this workflow change | ACP harness lib tests (`cargo test -p buzz-acp --lib`). Not covered by Desktop Rust — `desktop/src-tauri` does not depend on this crate |
| `macOS ARM Package` | Same desktop boundary as Desktop Fast | Unsigned `aarch64-apple-darwin` Tauri package with Nuncio identity |
| `Project Relay` | Project, relay, schema, or Nostr paths change | Kind `30617` local-path lifecycle against an isolated real relay |
| `Desktop Smoke E2E` | Desktop paths change | **nothing that blocks merge** — advisory (`continue-on-error`), excluded from the gate by design (#36/#37) |
| `Desktop E2E Integration` | Same desktop boundary as Desktop Smoke E2E | **nothing that blocks merge** — advisory (`continue-on-error`), two shards, real relay + Postgres/Redis/MinIO, `playwright --project=integration` (D-047 / #147). Includes the Crew-owned `evidence-reactions-relay` proof and inherited Buzz relay-backed specs |

### Desktop E2E Integration — expected failures (accepted upstream drift)

The lane stays advisory (D-047). A red Integration job is **not** merge-blocking,
but the expected-failure set must stay explicit so a new failure is visible.

Inventory from consistent fails across runs
[`31567147317`](https://github.com/Nuncio-hq/crew/actions/runs/31567147317)
(PR #165) and
[`31573328800`](https://github.com/Nuncio-hq/crew/actions/runs/31573328800)
(PR #168), reconfirmed on PR #176; Crew-owned strict-mode miss closed in #171.
Re-checked after the 0.5.18 children on run
[`32620379962`](https://github.com/Nuncio-hq/crew/actions/runs/32620379962)
(#319): catalog/create/share/emoji/discovery and live-mention home-feed
refetch now pass and are dropped. Remaining rows still failed on that run.

| Spec / case | Disposition | Notes |
| --- | --- | --- |
| `evidence-reactions-relay.spec.ts` — Accept/Reject strict-mode duplicate `evidence-reaction-rejected` | **Fixed** (#171) | Timeline-scoped card locator; mirrors smoke PR #170. Reject opens thread → dual card is product-correct. |
| `agents.spec.ts` — moves agent actions into an overflow menu in a narrow view | **Accepted upstream drift** | Still red on run 32620379962 after the 0.5.18 children. Do not open sprawl issues. |
| `profile.spec.ts` — runtime-tab respond-to; Inbox badge from notification settings | **Accepted upstream drift** | Still red on the same run. |

One-run / non-intersecting fails (e.g. occasional `onboarding.spec.ts` cases) are
**not** in this expected set — treat them as new signal until they appear on
multiple independent runs.

Do not weaken assertions, skip, or inflate timeouts to silence these. When
upstream lands matching fixes, drop the corresponding row here during the sync
PR that absorbs them.

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
