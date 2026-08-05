# Phase 03 — Profile lifecycle completion (Slice 4)

- **Status:** Not started — requires Phase 02 and manager approval
- **Contracts:** C-13, C-14 (+ C-03 repair path)

## Deliverable

Crew completes the hire/offboard loop it started in Phase 02:

1. **Create-in-place:** agent creation can run
   `hermes profile create <name> --no-alias` as an explicit, visible
   step (P-6); failure aborts agent creation with the CLI's error
   surfaced. Name validation mirrors `[a-z0-9][a-z0-9_-]{0,63}` before
   spawn.
2. **Offboarding choice (C-13/C-14):** deleting a Hermes agent asks
   keep/delete for the bound profile. Delete path always runs
   `hermes profile delete <name> -y` and verifies by directory absence
   (spike 0011: bare delete auto-cancels with exit 0 on non-TTY).
3. **Orphan repair (C-03 extension):** when spawn fails with the
   "profile does not exist" class, offer recreate-or-rebind instead of a
   dead badge.
4. **UX decision:** `--no-skills` (empty) vs bundled-skills default for
   Crew-created profiles — decide during implementation with manager
   input; record in DECISIONS.md if it locks.

## Security posture

- All CLI invocations are direct consequences of a manager click, shown
  with their literal command line (auditable, P-6).
- Public-agent gating stays: creation flow warns when
  `respond-to ≠ owner-only` is combined with a fallback-enabled profile
  (spike 0010) until the Hermes isolation switch exists.

## RED-first test contract

- Headless lifecycle driver tests against a temp HERMES_HOME (create,
  duplicate, invalid name, delete -y, delete-missing).
- Desktop flow tests for the keep/delete dialog and orphan repair.

## Exit criteria

C-13/C-14 GREEN; full hire→work→offboard loop demonstrated in a
verification record; runbook `HERMES.md` updated to mark the manual
steps as superseded.
