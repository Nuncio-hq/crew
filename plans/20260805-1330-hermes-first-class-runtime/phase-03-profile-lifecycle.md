# Phase 03 — Profile lifecycle completion (Slice 4)

- **Status:** COMPLETE (this branch)
- **Contracts:** C-13, C-14 (+ C-03 repair path)

## Deliverable

Crew completes the hire/offboard loop it started in Phase 02:

1. **Create-in-place:** ✅ Explicit button in create/edit Hermes binding
   field runs `hermes profile create <name> --no-alias` (auditable
   command line shown). Never silent on save. Failure surfaces CLI
   error class; name validated before spawn.
2. **Offboarding choice (C-13/C-14):** ✅ Agent delete confirm (profile
   panel) and persona delete confirm offer keep (default) vs also-delete.
   Delete always runs `hermes profile delete <name> -y` and verifies by
   directory absence (spike 0011 trap handled in Rust).
3. **Orphan repair (C-03 extension):** ✅ Readiness now checks profile
   directory existence; config-nudge row offers Recreate / Change binding.
4. **UX decision:** ✅ Keep bundled skills (no `--no-skills`) — see
   D-023. Agents benefit from the standard skill set; empty profiles
   remain a power-user CLI flow.

## Design decisions recorded

| Topic | Choice |
| ----- | ------ |
| `--no-skills` | **No** (bundled skills default) — D-023 |
| List profiles | Directory read of `~/.hermes/profiles/` (or `$HERMES_HOME/profiles`), not CLI |
| Delete gate | Exit 0 **and** directory absent; exit-0-but-present → Failed |
| Missing delete | AlreadyGone (success-ish) |
| Public-agent warning | Display-only on create-in-place + offboard when respond-to ≠ owner-only |
| `default` | Hard-rejected at service layer (create + delete) |

## Security posture

- All CLI invocations are direct consequences of a manager click, shown
  with their literal command line (auditable, P-6).
- Public-agent gating stays: creation/offboard flows warn when
  `respond-to ≠ owner-only` is combined with a fallback-enabled profile
  (spike 0010) until the Hermes isolation switch exists.

## RED-first test contract

- ✅ Headless lifecycle driver tests against a temp `HERMES_HOME` with a
  fake `hermes` script: create success/invalid/duplicate/missing-binary;
  delete success; exit-0-but-dir-present fails; missing → already-gone;
  `default` rejected.
- ✅ Desktop E2E: create-in-place button; delete dialog keep/delete
  defaults to keep.
- ✅ Contract tests for command-line helpers + public-agent warning gate.

## Exit criteria

C-13/C-14 GREEN; `HERMES.md` updated; D-023 appended.
