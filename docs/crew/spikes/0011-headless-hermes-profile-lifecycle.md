# Spike 0011 — Headless Hermes profile lifecycle (create/delete)

- **Status:** PASS (with one S-4.2 correction)
- **Date:** 2026-08-05
- **Feature:** [`../features/0001-hermes-first-class-runtime.md`](../features/0001-hermes-first-class-runtime.md) (S0-3)

## Question

Do `hermes profile create <name>` and `hermes profile delete <name>` run
headlessly (no TTY prompts) with reliable exit codes, and what does a
fresh profile actually contain?

## Decision affected

P-6 (explicit profile lifecycle from Crew UI), Slice 4 (create-in-place,
keep/delete flow), C-13/C-14, and S-4.2/AC1 (fresh profiles minimal).

## Hypothesis

`create` is non-interactive; `delete` prompts unless `-y`; fresh
profiles are minimal and credential-empty.

## Scope

- Commands: `hermes profile create|delete`, stdin redirected from
  `/dev/null` to simulate a UI-spawned process.
- One disposable profile (`crewspike`).

## Exclusions

- `profile export/import/rename/alias`.
- Distribution-based profiles (`profile install`).

## Pass criteria

Create/delete succeed headlessly with distinct exit codes for success,
invalid input, duplicate, and missing target; fresh-profile contents
enumerated.

## Fail criteria

Any TTY hang, or exit codes that cannot distinguish outcome classes.

## Environment

Hermes v0.20.0 (2026.8.3), macOS 26.5.2.

## Method

Run each command with `</dev/null`, capture output and `$?`; inspect the
created directory; probe credential visibility from the fresh profile;
delete and re-delete.

## Results

| Operation | Output | Exit |
| --------- | ------ | ---- |
| `create crewspike --no-alias --description ...` | created; 71 bundled skills synced | 0 |
| `create "badname!"` | `Error: Invalid profile name ... [a-z0-9][a-z0-9_-]{0,63}` | 1 |
| `create crewspike` (duplicate) | `Error: Profile 'crewspike' already exists at <path>` | 1 |
| `delete crewspike` (no `-y`, stdin=/dev/null) | `Type 'crewspike' to confirm:` then `Cancelled.` — profile intact | **0** |
| `delete crewspike -y` | `Profile 'crewspike' deleted.` — directory gone | 0 |
| `delete crewspike -y` (missing) | `Error: Profile 'crewspike' does not exist.` | 1 |
| `hermes -p crewspike acp` (profile missing) | `Error: Profile 'crewspike' does not exist. Create it with: hermes profile create crewspike` | 1 |

Fresh profile contents (`--no-alias` create): `.env` (empty of values),
`profile.yaml` (description), `SOUL.md`, `cron/` (empty), `home/`,
`logs/`, `memories/`, `plans/`, `sessions/`, `skills/` (71 bundled),
`skins/`, `workspace/`. No `config.yaml` until first `config set`; no
gateway config; no `auth.json` until first agent run.

**S-4.2 correction (from spike 0010):** although the fresh profile
stores no credentials of its own, the credential pool falls back
read-only to the manager's global root store — `hermes -p crewspike auth
list` showed the manager's pooled OAuth credentials. "Fresh profile" ≠
"credential-isolated profile". Also note `create` without `--no-skills`
syncs 71 bundled skills — not empty, by design.

The name rule `[a-z0-9][a-z0-9_-]{0,64}` (observed in the error message)
gives Crew a validation regex to mirror before spawn (edge case §10 of
the feature doc).

## Edge cases observed

- `delete` without `-y` on a non-TTY exits **0** after auto-cancel — a
  Crew orchestration must always pass `-y` and must never treat exit 0 of
  a bare `delete` as "deleted"; verify by directory absence or use `-y`
  exclusively (C-14 test material).
- Missing-profile spawn gives the exact distinct error class C-03 needs
  (exit 1 + actionable message).
- `create` prints a next-steps banner including the alias name even with
  `--no-alias`; harmless.

## Limitations

- `--clone`/`--clone-all`/`--clone-from` semantics untested (future
  "template profile" flows).
- Windows untested.

## Verdict

**PASS** — lifecycle is fully headless with distinguishable outcomes.
Crew orchestration rules derived: always `-y` on delete; mirror the name
regex; use `--no-alias` (Crew binds by name, wrapper scripts violate
P-5); decide `--no-skills` vs bundled default in Slice 4 UX; document
that credential isolation requires a separate step (S-4.2 rewrite).

## Follow-up test contract

C-13/C-14 tests drive create/delete through the exact flag set above and
assert directory presence/absence, not just exit codes.

## Cleanup

`crewspike` deleted; no repo changes; temp outputs under `/tmp` removed
with the session.
