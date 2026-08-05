# Spike 0010 — Headless Hermes auth/readiness probe semantics

- **Status:** FAIL (probe shape exists; exit-code semantics do not)
- **Date:** 2026-08-05
- **Feature:** [`../features/0001-hermes-first-class-runtime.md`](../features/0001-hermes-first-class-runtime.md) (S0-2)

## Question

Does any current Hermes command distinguish "authenticated" from "not
authenticated" per profile, headlessly, **by exit code** — the contract
Buzz's tier-1 `auth_probe_args` requires?

## Decision affected

The upstream tier-1 entry's `auth_probe_args` field (§7.2 of the feature
doc) and the depth of C-12 (unauthenticated-profile UX). If no
exit-code-truthful probe exists, the upstream PR ships with
`auth_probe_args: None` and Crew readiness degrades to binary+profile
checks until Hermes adds one.

## Hypothesis

`hermes auth status <provider>` looked like the candidate; suspected but
unverified that it always exits 0.

## Scope

- Commands: `hermes auth status`, `hermes auth list`, `hermes doctor`,
  `hermes status`, `hermes acp --check`, with and without `-p`.
- Boundary: read-only probes only; no logout/login state mutation on the
  manager's real credentials.

## Exclusions

- Does not design the future probe (that is a Hermes-side ask).
- Does not test provider-token *validity* (expired-but-present tokens).

## Pass criteria

A documented command that exits 0 when the profile's configured provider
is authenticated and non-zero when it is not, without a TTY.

## Fail criteria

All candidate commands exit 0 regardless of auth state.

## Environment

- Hermes: v0.20.0 (2026.8.3), macOS 26.5.2
- Auth class: pooled OAuth credentials on the default store

## Method

Run each candidate in both states. For the "logged out" state, use a
provider id with no credentials (`hermes auth status
nonexistent-provider-xyz`) rather than logging out a real provider.
Confirm the human-readable output paths in source
(`hermes_cli/auth_commands.py:509` — `auth_status_command`).

## Results

| Command | State | Output | Exit |
| ------- | ----- | ------ | ---- |
| `hermes auth status openai-codex` | logged in | `openai-codex: logged in` | 0 |
| `hermes auth status anthropic` | logged in | `anthropic: logged in` | 0 |
| `hermes auth status nonexistent-provider-xyz` | logged out | `nonexistent-provider-xyz: logged out` | **0** |
| `hermes -p crewspike auth status openai-codex` | (see below) | `logged in` | 0 |
| `hermes acp --check` | n/a | `Hermes ACP check OK` (deps only, no auth) | 0 |
| `hermes doctor` | 2 issues found | issue list printed | **0** |

Source confirms: `auth_status_command` prints and returns; it never sets
a non-zero exit for the logged-out branch
(`hermes_cli/auth_commands.py:514-520`).

Buzz's probe contract is exit-code-only: `login_probe` maps
`status.success()` → LoggedIn, everything else → LoggedOut
(`desktop/src-tauri/src/managed_agents/readiness/cli_probe.rs:68-72`).
Parsing stdout text is not part of that contract, and the only
stderr-classification path is for config-parse signals.

**Additional finding (affects S-4.2 and C-12):** the fresh `crewspike`
profile reported `openai-codex: logged in` and `hermes -p crewspike auth
list` showed the manager's pooled credentials. Fresh profiles **fall back
to the global root credential store read-only** (see
`agent/credential_pool.py` — global-root fallback comments around
`load_pool`, and `_write_through_xai_oauth_to_global_root`). So
"profile not authenticated" barely occurs on the manager's own machine —
but this also means a Crew-provisioned profile is *not* credential-empty
by default (the manager's provider credentials are readable by the
agent).

## Edge cases observed

- Provider ids are free-form: a typo probes as "logged out" rather than
  erroring — a text-parsing readiness check could silently probe the
  wrong provider.
- `hermes doctor` exits 0 even with issues found, so it is not usable as
  a readiness gate either.
- The probe would also need the *profile's configured provider* as input;
  today that requires `hermes -p X config get model.provider` first
  (exit-code-truthful for missing keys was not tested).

## Limitations

- Did not test a real logged-out provider with expired/revoked tokens.
- Hermes version-pinned: a newer Hermes may add `--check` semantics.

## Verdict

**FAIL** — no current command satisfies the exit-code contract.
Consequences applied to the feature plan:

1. Upstream tier-1 PR ships `auth_probe_args: None` +
   `login_hint: Some(...)` until Hermes provides a probe.
2. Hermes-side ask (feature doc §7.3) is now concrete:
   `hermes auth status <provider> --check` (or similar) exiting non-zero
   when logged out, honoring `-p`.
3. C-12 in Crew degrades to: binary present + profile exists (+ model
   configured via `config get`), with auth surfaced only reactively.
4. The credential global-root fallback finding rewrites S-4.2/AC1: fresh
   profiles are NOT credential-isolated. Public/anyone-facing agents need
   a documented credential-isolation step, and the feature doc's
   "profiles start minimal" claim must be corrected.

## Follow-up test contract

When a probe exists: readiness test asserting LoggedIn/LoggedOut mapping
from real exit codes for a Hermes runtime entry. Until then: C-12 tests
assert the degraded (binary+profile) readiness path and its copy.

## Cleanup

No state mutated; no credentials recorded. Table above contains no
secret values.
