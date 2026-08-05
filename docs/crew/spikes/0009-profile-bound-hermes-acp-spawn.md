# Spike 0009 — Profile-bound Hermes ACP spawn (`hermes -p <profile> acp`)

- **Status:** PASS
- **Date:** 2026-08-05
- **Feature:** [`../features/0001-hermes-first-class-runtime.md`](../features/0001-hermes-first-class-runtime.md) (S0-1)

## Question

Does an ACP client spawning `hermes -p <profile> acp` complete a real
initialize → session/new → prompt → reply turn, with all session state
landing in the named profile and none in the manager's default
`~/.hermes` store — and does the existing `buzz-acp` Hermes env guard
(C-17) plus arg passthrough hold for this command shape?

## Decision affected

P-1 (agent = profile), P-5 (spawn identity `hermes`/`hermes-acp` only),
C-02 (profile-bound spawn), C-17 (env guard). If this fails, the whole
profile-per-agent model needs a different selection mechanism
(`HERMES_HOME` env var) or is infeasible.

## Hypothesis

`-p` is stripped pre-argparse and sets `HERMES_HOME` before imports, and
the ACP adapter resolves everything through `get_hermes_home()`, so the
handshake should work and state should isolate. Args should pass through
because neither `buzz-acp` nor desktop discovery defines default args for
the `hermes` identity.

## Scope

- Components: Hermes Agent v0.20.0 ACP adapter, direct stdio JSON-RPC
  probe (no relay, no buzz-acp process — the uncertain boundary is the
  Hermes side of the ACP contract).
- Files: probe script archived at
  [`assets/0009-acp-handshake-probe.py`](assets/0009-acp-handshake-probe.py).
- Boundary: one disposable profile (`crewspike`), trivial prompts, one
  afternoon.

## Exclusions

- No relay round-trip (mention → reply through buzz-acp) — that is the
  Slice 1 live probe.
- No Windows.
- Does not prove same-process model pickup across sessions (see
  Limitations).

## Pass criteria

1. `initialize` and `session/new` return valid results.
2. A prompt returns an `agent_message_chunk` with the requested reply and
   `stopReason: end_turn`.
3. Session artifacts (session dumps, `state.db`) appear under
   `~/.hermes/profiles/crewspike/`; root `~/.hermes/state.db` mtime is
   unchanged and the default profile's configured model is untouched.
4. `buzz-acp` unit tests confirm the env guard matches bare `hermes`, and
   both `default_agent_args` implementations return `None` for it (args
   pass through).

## Fail criteria

Handshake error, adapter crash, state written to the root store, or args
mangled for the `hermes` identity.

## Environment

- Commit: `40773ea6d` (crew main)
- OS: macOS 26.5.2 (arm64)
- Hermes: `Hermes Agent v0.20.0 (2026.8.3)`; `hermes acp --version` → 0.20.0
- Auth class: pooled OAuth credentials (device-code / OAuth), no secrets
  recorded

## Method

1. `hermes profile create crewspike --no-alias` (see spike 0011).
2. Run the archived probe: newline-delimited JSON-RPC over stdio —
   `initialize` (protocolVersion 1) → `session/new` (cwd, empty
   mcpServers) → `session/prompt` ("Reply with exactly the word: pong").
   `HERMES_ACP_SKIP_CONFIGURED_MCP=1` set to mirror `buzz-acp`'s
   `default_agent_env`.
3. Record root `~/.hermes/state.db` mtime before/after; list profile
   `sessions/`; read default profile's `model.default` after.
4. `cargo test -p buzz-acp default_agent_env` and inspect
   `default_agent_args` in `crates/buzz-acp/src/config.rs:693` and
   `desktop/src-tauri/src/managed_agents/discovery.rs:462`.

## Results

- `initialize` → `agentInfo: {name: "hermes-agent", version: "0.20.0"}`,
  capabilities `loadSession`, `promptCapabilities.image`,
  `sessionCapabilities` fork/list/resume, plus advertised `authMethods`.
- `session/new` → sessionId, `_meta.hermes.sessionProvenance`, and a
  `models.availableModels` catalog (multi-provider) — the payload Crew
  can display as "current model" (S-3.2/AC2).
- Prompt → reply `pong`, `stopReason: end_turn`,
  `usage: {inputTokens: 20141, outputTokens: 5}`.
- Isolation: profile dir gained `state.db` (+wal/shm), `auth.json`,
  session request dumps; `sessions` table count 3 after runs. Root
  `~/.hermes/state.db` mtime identical before/after (`1785898596`);
  default profile model still `openai-codex/gpt-5.6-sol`.
- Model-from-profile chain verified end-to-end:
  - blank profile model → per-turn provider error surfaced as reply
    ("model: String should have at least 1 character"), `end_turn`, no
    crash — an unconfigured profile fails soft;
  - profile set to an Anthropic model → Anthropic billing error surfaced
    (proves the profile's provider was hit);
  - profile set to `openai-codex/gpt-5.6-sol` → `pong`.
- C-17: `config::tests::default_agent_env_recognizes_hermes_identities`
  passes and its fixture list includes bare `hermes`
  (`crates/buzz-acp/src/config.rs:1637`). Both `default_agent_args`
  implementations return `None` for the `hermes` identity, so
  `normalize_agent_args` passes `-p crewspike acp` through unchanged.

## Edge cases observed

- The adapter's first-run "Install browser tools?" prompt is TTY-gated
  (`acp_adapter/entry.py:177` returns when stdin is not a TTY) — safe for
  headless spawns.
- An unconfigured profile model degrades to a readable in-channel error,
  not a hang or crash — good failure mode for C-12's "unready profile"
  class.

## Limitations

- Model changes were observed across *process* restarts (each probe run
  spawns a fresh adapter). Whether a long-lived adapter picks up a
  profile model change at its next `session/new` without respawn (C-07
  strict reading) is unproven.
- No relay in the loop; buzz-acp's own spawn path is exercised only at
  the unit-test level here.

## Verdict

**PASS** — the profile-bound spawn shape works end-to-end, state isolates
to the profile, and the guard/arg behavior needed by P-5/C-17 is already
in place.

## Follow-up test contract

C-02 (live relay mention → reply via buzz-acp with `-p`), C-07 (model
pickup without respawn), C-17 (integration-level env probe with args
`-p X acp`) must be RED before Slice 2 implementation.

## Cleanup

`/tmp` probe scripts archived to `assets/` and removed; `crewspike`
profile deleted via `hermes profile delete crewspike -y` (spike 0011
records the deletion evidence). No repo code changed.
