---
phase: 04
title: Archive, restore, permanent delete, running-agent guard
status: planned
priority: P0
effort: L (3-4 d)
dependencies: ["01"]
---

# Phase 04 — Archive, restore, permanent delete, running-agent guard

## Outcome

Offboarding stops destroying an employee record. The backend can file a
profile away with a manifest, bring it back, and — only for something
already filed away — destroy it deliberately. Every destructive path
refuses while the agent is running.

DoD coverage: #3 (backend half), #4 (authoritative half).

## Design constraints inherited from the plan

- **DD-4:** the running-agent guard **refuses** while a runtime pair is
  alive. It does not stop the agent for the owner. The issue's
  "existing graceful stop machinery: SIGTERM → wait → SIGKILL fan-out"
  does not exist — `managed_agents/runtime/stop.rs:40,120,153` uses
  `Child::kill()` (immediate SIGKILL), and the only `SIGTERM` in
  `managed_agents` is `discovery.rs:927`, inside the auth-probe
  timeout. Building graceful stop is out of scope for #119.
- **DD-5 / OQ-1 / OQ-2:** archive location, format, and exclusion list
  come from the phase-01 spike record. This phase **may not** invent
  them.
- The guard is authoritative **in Rust**. UI disabling (phase 06) is
  advisory only — a Playwright-invisible path must not be able to
  corrupt a live profile (plan R8).
- **DD-7:** RED tests first, failure output in the PR body.

## Seams

| Seam | Use |
| ---- | --- |
| `managed_agents/hermes_profile_lifecycle.rs` — `HermesProfileLifecycleResult` | Named-result philosophy: every new op returns named states with human copy, never a bool |
| `hermes_profile_lifecycle.rs` — `hermes_home()`, `hermes_profiles_dir()`, `hermes_profile_dir()`, `list_profiles()` | Path resolution and listing — reuse, never re-derive |
| `managed_agents/hermes_profile.rs:13` (`HERMES_FORBIDDEN_PROFILE_NAME = "default"`), `:19` (`validate_hermes_profile_name`) | Every new destructive path routes through both |
| `commands/hermes_profiles.rs:11,17,29` | Where the new IPC commands live, beside `list`/`create`/`delete` |
| `lib.rs:795-797` | Three additive `invoke_handler` registration lines |
| `managed_agents/runtime/stop.rs`, `runtime_commands.rs:313` | Liveness read for the guard (per phase-01 I6) |
| `src-tauri/src/util.rs:86,242,275` | Reference for restricted file permissions on archive artifacts |

## Work

1. **RED contract tests** (write first, observe failure, record):
   - Archive round-trip: archive → the live profile directory is gone,
     the archive exists, the manifest parses.
   - Manifest content: profile name, archive timestamp, bound agent name
     + pubkey, optional free-text reason.
   - Cache exclusion: caches present before archive are absent from the
     archive; non-cache content survives byte-identical.
   - Size estimate is produced before the action and is within a stated
     tolerance of the real archive.
   - Restore: unpacks to `~/.hermes/profiles/<name>` with content
     intact.
   - Collision: restore onto a live profile of the same name is
     **refused** with a named result, and the live profile is untouched.
   - Permanent delete: succeeds on an archive; is **not exposed** for a
     live profile; requires the type-name confirmation token.
   - Guard: archive / restore-over / permanent-delete are refused while
     a runtime pair is alive, with a named reason. Repeat after stop →
     succeeds.
   - `default` is hard-rejected on every new path; invalid names
     rejected before any filesystem write.
   - Path-traversal guard: a manifest or archive name containing
     `..`/separators cannot escape the archive area or the profiles dir.

2. **Archive service** (Crew-owned Rust module): pack per the phase-01
   format decision, applying the phase-01 exclusion list; write the
   manifest; compute the size estimate; restricted permissions per S18.
   Never touch `~/.hermes` root, never touch `default`.

3. **Restore service**: read manifest, collision-check against
   `list_profiles()`, unpack, return a named result that carries enough
   for the UI to offer re-bind.

4. **Permanent delete**: operates on an archive identifier only. The
   signature takes the confirmation token so the backend, not the
   dialog, is the gate.

5. **Guard**: a shared precondition used by all three destructive ops,
   reading liveness per phase-01 I6. Named refusal result carrying the
   reason and the agent identity.

6. **IPC commands** in `commands/hermes_profiles.rs` (Crew-owned) +
   three registration lines in `lib.rs` (upstream, ~+3 lines) +
   invoke wrappers in `shared/api/hermesProfiles.ts` (Crew-owned,
   beside `:42,48,56`).

7. **`docs/crew/STATE.md`** updated in this PR (#117).

## Files

- **Create (Crew-owned):** archive/restore/permanent-delete service
  module(s) under `desktop/src-tauri/src/managed_agents/` + tests
- **Modify (Crew-owned):** `commands/hermes_profiles.rs`,
  `shared/api/hermesProfiles.ts`
- **Modify (upstream, justified):** `lib.rs` (3 registration lines)
- **Read only:** `hermes_profile_lifecycle.rs`, `hermes_profile.rs`,
  `runtime/stop.rs`, `util.rs`, phase-01 spike record
- **Must not touch:** `usePersonaActions.ts` and the dialogs (phase 06),
  the create flow, occupancy checks, owner-only/local invariants

## Validation

- All RED tests green, run against a temp `HERMES_HOME` with
  `lock_path_mutex()`, matching the existing lifecycle test pattern.
- Guard test proves refusal-while-running is enforced in Rust with no
  UI involved.
- `cargo test --manifest-path desktop/src-tauri/Cargo.toml` green.
- `node desktop/scripts/check-file-sizes.mjs` passes; split by
  responsibility (D-022) if a module grows — never raise `MAX_LINES`.
- No `unsafe`, no new `unwrap()`/`expect()` in production paths.
- `just ci` green.

## Risk and rollback

- **Risk (highest in the plan):** a bug here destroys real profile
  state. Mitigation: archive is **copy-then-verify-then-remove**, never
  move-then-hope; the live directory is removed only after the archive
  is written and re-read successfully. Any failure leaves the live
  profile intact and returns a named failure.
- **Risk:** the archive area fills the disk over time. Out of scope to
  manage (scheduled backups are a stated non-goal), but the manifest
  and size estimate make the cost visible; note retention as a
  follow-up.
- **Risk:** exclusion list drifts as Hermes adds cache dirs. Mitigation:
  the manifest records the exclusion list actually applied, so a stale
  list is diagnosable from any archive.
- **Rollback:** the commands are additive and unreferenced until phase
  06 wires the UI. Reverting removes capability without changing any
  shipped flow.

## PR

Branch `agents/profile-archive`. Target `Nuncio-hq/crew` (D-020).
`git commit -s`. PR body records the RED output and an archive →
restore transcript against a scratch profile.
