# Verification 0007 — Hermes profile lifecycle hardening

- **Date:** 2026-08-10
- **Feature:** Issue #119 readiness, offboarding, and archive lifecycle
- **Decision:** D-035; spike 0015
- **Boundary:** Crew-owned Hermes readiness projection, mock-bridge desktop
  surfaces, and real Rust profile archive contracts.

## Setup

The desktop E2E build was produced with `pnpm --filter buzz build:e2e`.
Playwright ran headlessly under Xvfb against the deterministic mock bridge:

```text
Running 4 tests using 1 worker
4 passed (13.7s)
```

The Rust contract tests ran against temporary filesystem roots and injected
runtime/profile state. No production Hermes installation was assumed.

## DoD evidence

### Named readiness states — PASS

`hermes-profile-lifecycle.spec.ts` seeds each named state through the mock
bridge and asserts its state-specific test ID and copy:

- `ready` — `hermes-readiness-ready`
- `missing` — `hermes-readiness-missing`
- `broken_config` — `hermes-readiness-broken-config`
- `binary_missing` — `hermes-readiness-binary-missing`
- `auth_unknown` — `hermes-readiness-auth-unknown`, neutral styling, and
  “Auth not verifiable” advisory copy.

The five state screenshots and raw run output are attached below.

### Offboarding — PASS

The Playwright spec verifies keep is selected by default, archive is not
preselected, the running-agent archive control is disabled with stop-first
copy, and captures the dialog. A stopped-agent fixture selects archive and
asserts the estimate and optional reason field are rendered.

### Archive listing, restore, collision, re-bind, and permanent delete — PASS

The mock bridge seeded a manifest-bearing archive. The spec verified manifest
facts, captured the listing, restored it successfully, verified the focused
re-bind offer for the still-present bound agent, then recreated the profile
and verified the backend collision message:

```text
profile already exists
```

Permanent deletion remains disabled until the exact profile name is typed;
the confirmation state was captured as a screenshot.

### Rust readiness contracts — PASS

Command:

```text
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'readiness::hermes' -- --nocapture
```

Excerpt:

```text
running 7 tests
test managed_agents::readiness::hermes::tests::readiness_contract_names_all_states_and_keeps_auth_unknown_advisory ... ok
test managed_agents::readiness::hermes::tests::healthy_profile_is_auth_unknown_without_a_requirement ... ok
test managed_agents::readiness::hermes::tests::readiness_fixture_maps_missing_broken_and_binary_states ... ok
test result: ok. 7 passed; 0 failed
```

### Rust archive contracts — PASS

Command:

```text
cargo test --manifest-path desktop/src-tauri/Cargo.toml hermes_profile_archive -- --nocapture
```

Excerpt:

```text
running 7 tests
test managed_agents::hermes_profile_archive::tests::archive_restore_round_trip_and_cache_exclusion ... ok
test managed_agents::hermes_profile_archive::tests::corrupt_sidecar_is_skipped_by_archive_listing ... ok
test managed_agents::hermes_profile_archive::tests::running_agent_guard_matches_profile_and_runtime_pair ... ok
test managed_agents::hermes_profile_archive::tests::invalid_name_and_confirmation_are_rejected ... ok
test result: ok. 7 passed; 0 failed
```

## Distinct-state screenshot gate

All eight PNG hashes are distinct:

| Screenshot | SHA-1 |
| --- | --- |
| `archives-list.png` | `54abcfc0296b4629aaa6c7d8989018ecc4dc5da9` |
| `offboard-archive-dialog.png` | `5cc54af016c30b0ec71bef7e124ffcb094e6ec72` |
| `permanent-delete-confirmation.png` | `c03fbd2c08d61b56a7c34212a41d5f47c99ef775` |
| `readiness-auth_unknown.png` | `8dbc53357533f721a0616a033efd816949245174` |
| `readiness-binary_missing.png` | `7b1042d046ec810f727d817f1b11cabcbb32e6d8` |
| `readiness-broken_config.png` | `5b2d8f857f0d5afe33a905ae20b4570a7bbcf00a` |
| `readiness-missing.png` | `b8a2f0d235dd91b763e4b77f31833939265c51e5` |
| `readiness-ready.png` | `b8ff894ceb332a7e7fa20114017e4c2d249a2e07` |

The source output is `/home/ubuntu/119-evidence/screenshots-shasum.txt`.

## Not verifiable in this environment

There is no real Hermes binary installed here. Therefore a truthful
headless Hermes authentication probe and a live Hermes spawn cannot be
verified. A Hermes-equipped machine must provide the real binary, a
headless authentication probe, and a disposable profile to exercise that
boundary. The readiness, filesystem, archive, guard, and UI contracts above
do not claim that unavailable probe.

## Artifacts

- `/home/ubuntu/119-evidence/playwright-run.txt`
- `/home/ubuntu/119-evidence/screenshots-shasum.txt`
- `/home/ubuntu/119-evidence/readiness-ready.png`
- `/home/ubuntu/119-evidence/readiness-missing.png`
- `/home/ubuntu/119-evidence/readiness-broken_config.png`
- `/home/ubuntu/119-evidence/readiness-binary_missing.png`
- `/home/ubuntu/119-evidence/readiness-auth_unknown.png`
- `/home/ubuntu/119-evidence/offboard-archive-dialog.png`
- `/home/ubuntu/119-evidence/archives-list.png`
- `/home/ubuntu/119-evidence/permanent-delete-confirmation.png`
- `/home/ubuntu/119-evidence/rust-readiness-nocapture.txt`
- `/home/ubuntu/119-evidence/rust-archive-nocapture.txt`
