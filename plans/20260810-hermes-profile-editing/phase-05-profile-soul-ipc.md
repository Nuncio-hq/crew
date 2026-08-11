---
phase: 05
title: SOUL.md read/write/reset IPC
status: planned
priority: high
effort: M
dependencies: [01, 02]
---

# Phase 05 — SOUL.md read/write/reset IPC

Issue #118 thing-to-solve 2, backend half. Layer 1 (the profile's own persona
document) becomes readable and writable from Crew.

## Why this matters (evidence)

Every real profile on the manager's machine has a **one-line `SOUL.md`** —
`builder`, `scout`, and `crewmission` all still carry the generic default. The
issue's claim that "nobody edits it, so they all sound generic" is corroborated
on disk, not assumed.

## Deliverable

| Command | Shape |
| ------- | ----- |
| `read_hermes_profile_soul(name)` | current file content, or a named failure |
| `write_hermes_profile_soul(name, content)` | result enum; content written verbatim |
| `reset_hermes_profile_soul(name)` | restores the Hermes default — **only if P01/Q3 found a trustworthy source** |

## Hard product rule (from the issue)

> Edit-in-place on the **real current content**. **Never a blank-replace box.**

The editor must open populated. An unreadable file is a named failure, not an
empty string — presenting empty content as "the persona" invites the founder to
save over real prose.

## Design

- Path: `hermes_profile_dir(name)` (`hermes_profile_lifecycle.rs:103`) joined
  with `SOUL.md`. `hermes_home()` at `:87` already honours the `HERMES_HOME`
  override, so tests can point at a temp dir.
- Existence check reuses `hermes_profile_directory_exists`
  (`hermes_profile_lifecycle.rs:108`) — a missing profile is `DoesNotExist`, and
  the command must **never create** a profile directory as a side effect.
- Name validation reuses `validate_hermes_profile_name` and the `default`
  rejection (`managed_agents/hermes_profile.rs`) before any filesystem access.
- Writes are atomic (temp file in the same directory + rename) so an interrupted
  save cannot leave a truncated persona.
- Read and write are byte-exact: no trailing-newline normalisation, no CRLF
  rewriting, no re-encoding.

### Reset source (unresolved question 1)

P01/Q3 decides one of:

| Source | Verdict |
| ------ | ------- |
| Template shipped inside the Hermes install | **Preferred** — Hermes stays the source of truth |
| Create a throwaway profile in a temp `HERMES_HOME` and copy its `SOUL.md` | Acceptable fallback; side-effect-free because it never touches `~/.hermes` |
| A copy bundled in Crew | **Rejected** — drifts from Hermes and would make Crew a second store |

If none is trustworthy, **drop `reset_hermes_profile_soul` and the reset button**
and record it as a known gap in `HERMES.md`. Edit-in-place still ships.

## Files

| Path | Owner | Change |
| ---- | ----- | ------ |
| `desktop/src-tauri/src/managed_agents/hermes_profile_soul.rs` | **new, Crew-only** | read/write/reset + result enum + `#[cfg(test)]` tests against a temp `HERMES_HOME` |
| `desktop/src-tauri/src/commands/hermes_profiles.rs` | Crew-only | up to three `#[tauri::command]` wrappers |
| `desktop/src-tauri/src/lib.rs` | **upstream** | **+2 or +3 lines** in `invoke_handler` next to `:795-797`. Same justification as P04 |
| `desktop/src/shared/api/hermesProfiles.ts` | Crew-only | TS wrappers |
| `desktop/tests/helpers/bridge.ts` | Crew-only test helper | seeded `SOUL.md` content + an unreadable-file failure mode |

## Security and privacy

- `SOUL.md` is founder-authored prose and may contain business context. It stays
  local: never published to a relay, never quoted in an issue or PR body, and
  never included in a posted screenshot without explicit founder approval.
- No `unsafe`; no new `unwrap()`/`expect()` on production paths — `?` and typed
  errors (root `AGENTS.md` quality gates).
- Never read any other file in the profile directory. `auth.json`, `memories/`,
  `sessions/`, `skills/`, and `plans/` are out of scope (issue non-goals).

## Turns green

E-04, E-05, E-06.

## Verification

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml hermes_profile_soul
just desktop-tauri-test
```

Manual: open `scout`'s persona in Crew, confirm it matches the file on disk
byte-for-byte, save an edit, confirm the file changed and nothing else in the
profile directory did.
