# Spike 0056 — Hermes default / home profile ACP spawn argv

**Verdict: PASS — `hermes -p default acp` starts ACP against `~/.hermes` and does not create `~/.hermes/profiles/default`. `hermes acp` (no `-p`) also works on this machine with no sticky `active_profile`.**

- **Status:** PASS
- **Date:** 2026-08-20
- **Issue:** [Nuncio-hq/crew#243](https://github.com/Nuncio-hq/crew/issues/243)

## Question

What argv actually runs Hermes ACP against the manager home profile
(`~/.hermes`) without creating `~/.hermes/profiles/default`?

Compared at least:

1. `hermes acp` (no `-p`)
2. `hermes -p default acp`

## Decision affected

Issue #243 spawn-bind of `hermes_profile: "default"` after confirmation.
D-019 item 7 originally required confirmation rather than a hard ban;
D-024 kept the named-profile requirement. Crew today injects
`hermes -p <bound name> acp` (HERMES.md rule 3). This spike decides
whether that shape is valid when `<bound name>` is `default`, or whether
Crew must special-case a bare `hermes acp`.

## Hypothesis

Hermes treats the id `default` as the home store (`~/.hermes`), not as
`~/.hermes/profiles/default`. Both argv should start ACP against the
home profile. `-p default` is expected to be the Crew-shaped spawn
because the desktop already injects `-p <name>`.

## Scope

- Real `hermes` on this Mac; live `~/.hermes` layout (empty `profiles/`).
- Handshake only: ACP `initialize` then `session/new`, then exit.
- Probe archived at
  [`assets/0056-home-profile-acp-probe.py`](assets/0056-home-profile-acp-probe.py).
- Raw evidence:
  [`assets/0056-home-profile-acp/evidence.json`](assets/0056-home-profile-acp/evidence.json),
  [`assets/0056-home-profile-acp/home-state-sessions.json`](assets/0056-home-profile-acp/home-state-sessions.json).

## Exclusions

- No `session/prompt` / agent turn.
- No production code, UI, DECISIONS rewrite, push/PR.
- Isolated `HERMES_HOME` was not used; live `~/.hermes` was the proof.
- Does not prove Crew occupancy, confirmation UI, or write-through
  rejects.
- Sticky `~/.hermes/active_profile` was absent here; bare `hermes acp`
  following a named sticky profile was not live-tested.

## Pass criteria

Declared before running (spike file committed to disk as RUNNING first):

- PASS: one argv starts ACP against the home profile (`~/.hermes`);
  no `~/.hermes/profiles/default` directory is created; the exact argv
  is recorded.
- Evidence must include filesystem snapshots (before/after
  `~/.hermes/profiles`) plus command output (initialize / session/new),
  not a model guess.

## Fail criteria

- FAIL: neither argv starts ACP against the home profile, or both
  create `~/.hermes/profiles/default`.
- If a command would create `~/.hermes/profiles/default`, that argv is
  stopped and recorded FAIL for that argv.

## Inconclusive criteria

- INCONCLUSIVE: Hermes ACP cannot be run here (missing binary or auth
  that blocks the handshake). Record what was tried.

## Environment

- Commit: `eeaaacb13` (`feat/issue-243-hermes-default-spike`, tracking
  `origin/main`)
- OS: macOS 26.5.2 (Darwin 25.5.0 arm64)
- Hermes: `Hermes Agent v0.20.4 (2026.8.18)`; `hermes acp --version` →
  `0.20.4`; binary `/Users/a1241968/.local/bin/hermes`
- `hermes acp --check` → exit 0, `Hermes ACP check OK`
- Auth class: initialize advertised `xai-oauth` runtime credentials
  (no secrets recorded)
- Live layout before any ACP: `~/.hermes/profiles/` existed and was
  empty; `~/.hermes/profiles/default` absent; no
  `~/.hermes/active_profile`; `hermes profile list` showed `◆default`
  with model `grok-4.6`; home `config.yaml` `model.default: grok-4.6`
  / `model.provider: xai-oauth`

## Method

1. Wrote this record with Status RUNNING and the pass/fail criteria
   above **before** any ACP spawn.
2. Snapshot live `~/.hermes/profiles` (empty), `which hermes`,
   `hermes version`.
3. Preflight `hermes acp --check` (not one of the argv under test).
4. Probe: for each argv, child env copied from the parent then
   `HERMES_HOME` and `HERMES_PROFILE` unset (Crew-like spawn) with
   `HERMES_ACP_SKIP_CONFIGURED_MCP=1`. Send `initialize` then
   `session/new` (`cwd=/tmp/issue243-spike-cwd`, empty `mcpServers`).
   Do not send `session/prompt`. Sample `lsof` for `state.db`. SIGTERM
   after handshake.
5. Stop-rule: if `~/.hermes/profiles/default` appeared, kill that argv
   and mark it FAIL. It did not appear.
6. Confirm session rows in home `~/.hermes/state.db`, then delete those
   two empty ACP sessions (`hermes sessions delete --yes <id>`).

## Results

`~/.hermes/profiles/default` was absent before, between, and after both
argv. `profiles/` listing stayed `[]`.

| argv | initialize | session/new | sessionId | lsof `state.db` | created `profiles/default` |
|------|------------|-------------|-----------|-----------------|----------------------------|
| `hermes acp` | ok, `hermes-agent` 0.20.4 | ok, 2.48s | `b70ef320-5058-4377-b410-7de3b78d189b` | `/Users/a1241968/.hermes/state.db` (+wal/shm) | no |
| `hermes -p default acp` | ok, `hermes-agent` 0.20.4 | ok, 2.09s | `18c8e311-73a7-4947-96ce-2de066aca3b3` | `/Users/a1241968/.hermes/state.db` (+wal/shm) | no |

Both stderr lines created an OpenAI client with
`provider=xai-oauth` `model=grok-4.6` (home `config.yaml`), then
`Created ACP session … (cwd=/tmp/issue243-spike-cwd)`.

Home `state.db` mtime moved during the first handshake
(`1787194491.627` → `1787194496.674`). Read-only SQLite after both
runs found both session ids in **home** `sessions` with
`source=acp`, `model=grok-4.6`, `profile_name=null`,
`model_config={"cwd": "/tmp/issue243-spike-cwd"}`. There is no
`profiles/default/state.db` because that directory was never created.

`ps eww` did not surface a child `HERMES_HOME` key (macOS `ps` env
parsing is lossy here). `lsof` + SQLite are the store-identity
evidence.

## Edge cases observed

- Hermes CLI lists a `◆default` row even when `~/.hermes/profiles/` is
  empty. `default` is the home store, not a `profiles/` name.
- Source (not live-tested): `_apply_profile_override()` in
  `hermes_cli/main.py` follows `~/.hermes/active_profile` only when
  **no** `-p` is given and the sticky name is not `default`.
  `resolve_profile_env("default")` in `hermes_cli/profiles.py` returns
  the hermes root and never `profiles/default`. Bare `hermes acp` is
  therefore sticky-profile-sensitive; `hermes -p default acp` is not.
- Child inherited unrelated parent `HERMES_*` keys (this builder is a
  Hermes session) except `HERMES_HOME` / `HERMES_PROFILE`, which were
  unset. That did not redirect the store: both processes opened
  `~/.hermes/state.db`.
- Unrelated warnings: browser `check_fn` tools unavailable this turn.
  Not a failure.

## Limitations

- Handshake only; no prompt/turn, no buzz-acp, no Crew occupancy.
- One Mac, Hermes 0.20.4, no sticky `active_profile`.
- `ps eww` could not print child `HERMES_HOME`; store identity is
  `lsof` + SQLite + directory snapshots.

## Verdict

**PASS** — both argv start ACP against the live home profile and
neither creates `~/.hermes/profiles/default`.

**Recommended Crew spawn argv:** `hermes -p default acp`

That matches the existing `-p <bound name>` injection when
`hermes_profile` is `"default"`. Bare `hermes acp` also works here, but
it is a special-case spawn and would follow a named sticky
`active_profile` if one is set later.

## Follow-up test contract

Before implementation:

1. RED: spawn-arg builder for bound profile `default` must produce
   `["-p", "default", "acp"]` (or an equivalent vector whose resolved
   command is `hermes -p default acp`), not a mkdir of
   `~/.hermes/profiles/default`.
2. RED: path helper used by write-through / archive / delete must still
   refuse `default` (issue #243: confirmation bind ≠ Crew editor of
   `~/.hermes`).
3. Do not implement around a world where `-p default` created
   `profiles/default` — that world was not observed.

## Cleanup

- Removed `/tmp/issue243-spike-cwd`.
- Deleted the two empty ACP sessions from the live home store:
  `hermes sessions delete --yes b70ef320-5058-4377-b410-7de3b78d189b`
  and `18c8e311-73a7-4947-96ce-2de066aca3b3`.
- Probe + evidence JSON remain in `docs/crew/spikes/assets/0056-*`.
- Live `~/.hermes/profiles/` still empty; `profiles/default` still
  absent. No isolated `HERMES_HOME` was created.
