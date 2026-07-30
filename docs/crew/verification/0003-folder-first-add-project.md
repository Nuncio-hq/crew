# Verification 0003 — Folder-first Add Project

- **Date:** 2026-07-30
- **Result:** PASS WITH MANUAL UI SMOKE PENDING
- **Scope:** Projects page creation from an existing local folder

## Manager-visible outcome

The Projects page now keeps its `+` menu when the relay has no Projects.
Choosing **Repository** opens the native folder picker first. After selection,
NuncioCrew shows the exact path, relay destination, and editable Project name.
Confirmation creates the canonical Project channel and publishes kind `30617`.

The old standalone `Local workspace / Create or register a Project you own
first` strip is no longer rendered.

## Relay and identity guarantees

- Project identity remains `(pubkey, d)`.
- The raw path is one `buzz-location/local` metadata tag.
- The Project channel is one canonical `buzz-channel` tag.
- Duplicate `(owner, d)` is rejected before channel creation.
- Cache insertion happens only after relay acknowledgement and exact read-back.
- A channel created before a failed publication is reused for the same Project
  identity on retry.
- No `clone` tag is fabricated.

## Folder guarantees

Selected-folder access is picker-only. Identity lookup, relay operations, and
confirmed query-cache insertion occur elsewhere, but there is no folder
inspection, clone, checkout, `git init`, filesystem write, or Git operation.
Folder-picker cancel returns before all write paths.

## TDD evidence

The focused contracts were first run before production files existed:

```text
0 passed, 4 failed
```

After implementation:

```text
14 passed, 0 failed
```

They cover path/name derivation, Unicode and spaces, invalid paths, exact tags,
no clone metadata, exact duplicate identity, acknowledgement ordering,
full-identity retry and ACK recovery, read-side path/channel preservation,
clone/terminal suppression, malformed-metadata fail-closed behavior,
configured-checkout collision isolation, empty-state create access, shared
callback wiring, and strip removal.

## Normal gates

- Full desktop suite: `3840` passed, `1` gated live-relay test skipped,
  zero failed.
- TypeScript typecheck: pass.
- Biome and repository desktop checks: pass.
- File-size gate: pass.
- E2E web build: pass.
- Production NuncioCrew release build: pass.
- Rebuilt artifact:
  `desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/NuncioCrew.app`.

The build emitted only existing Rust dead-code and large-chunk warnings. Biome
reported two pre-existing informational notices in a persona catalog test.

## Limitation and pending smoke

Codex Computer Use verified the rebuilt real Tauri app through the
pre-publication boundary: the empty Projects page showed the `+` menu, the old
strip was absent, Repository opened the native picker, picker Cancel was inert,
and selecting `/Users/a1241968/Documents/NuncioADE` opened the review dialog
with the exact path, relay destination, and default name. The final Add Project
button was intentionally not pressed against the manager's real relay.

Arbitrary selected-folder `.git` and remote detection remain outside this
implemented slice. Spike 0006 subsequently proved that local snapshot reading
can reuse an existing Rust command for normal non-symlink paths; remote
detection remains separate and no Git wiring was added here.

Cross-client creation remains a low-probability race: two app instances can
both pass the exact duplicate preflight and create different channels before
the replaceable Project event converges. Deterministic channel creation or
relay compare-and-swap support is required to remove that race completely.
