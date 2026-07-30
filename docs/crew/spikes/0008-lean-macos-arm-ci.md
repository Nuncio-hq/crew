# Spike 0008 — Lean macOS ARM CI

## Question

Can Crew replace the inherited Buzz PR matrix with an additive, meaningful
gate for one-manager macOS Apple Silicon development without weakening the
relay-native Project boundary?

## Pass criteria

- Existing Buzz workflow files remain unchanged for upstream sync.
- Normal PRs need only desktop checks, one unsigned macOS ARM64 package build,
  and a focused real-relay Project test when relevant paths change.
- Web, mobile, Windows, Linux containers, and Kubernetes do not run
  automatically.
- A stable final check can be the only required branch-protection check.
- Core root and desktop Tauri Rust format, lint, unit, and dependency-policy
  checks remain available as a manual workflow; full platform and integration
  compatibility is not claimed.
- The PR package build does not require signing credentials or optional
  mesh-llm native libraries.

## Evidence

- The current PR passed Desktop Core, all four desktop smoke shards, and both
  relay-backed integration shards.
- Its four Docker failures occurred while exporting cache to Block-owned GHCR
  repositories, not while compiling Crew.
- The inherited macOS job failed before app compilation because its mesh-llm
  checkout locator found no directory after `cargo fetch`.
- `desktop/src-tauri/Cargo.toml` enables only `system-keyring` by default;
  `mesh-llm` is an explicit optional feature.
- Both `scripts/build-nuncio-crew-local.sh` and the reviewed
  `nuncio-crew-release.yml` build the Tauri app without `mesh-llm`.
- The existing live Project test publishes kind `30617`, reconnects, relinks a
  Unicode path, and resolves the latest path from a real isolated relay.
- GitHub currently registers only the inherited `CI` and `Docker image`
  workflows in the fork, so they can be disabled after the additive Crew gate
  is present on `main`.

## Limits

- A placeholder-sidecar PR package proves Tauri packaging but not the final
  signed sidecar payload. The manual release workflow builds real sidecars.
- A workflow contract cannot prove GitHub runner labels or repository rules;
  those need one real PR run and a live ruleset inspection.
- Signed, notarized, clean-install, and updater behavior remain release-stage
  proofs.

## Verdict

**PASS**

Use an additive `NuncioCrew CI`, one always-present `NuncioCrew Gate`, and a
manual upstream-sync validation workflow. Disable inherited automatic
workflows at repository level only after the new gate passes on the PR.
