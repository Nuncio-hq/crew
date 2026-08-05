# Verification 0006 — Hermes Slice 1: live relay round-trip on a profile-bound agent

- **Date:** 2026-08-05
- **Feature:** [`../features/0001-hermes-first-class-runtime.md`](../features/0001-hermes-first-class-runtime.md), Slice 1
- **Decision:** D-019
- **Commit:** `40773ea6d` (docs-only slice; no production code changed)

## Boundary exercised

Real Postgres+Redis (Docker, ports remapped 15432/16379 to avoid host
conflicts), real `buzz-relay` (`0.0.0.0:3000`), real `buzz-acp` release
binary, real Hermes Agent v0.20.0 spawned as `hermes -p crewspike acp`, and
the `buzz` CLI as the owner. No mocks anywhere.

## Setup evidence

- Relay signing key set (`BUZZ_RELAY_PRIVATE_KEY`); owner + agent keypairs
  minted with `buzz-admin generate-key`; both registered via `add-member`.
- Disposable profile `crewspike` (`--no-skills`), model
  `openai-codex/gpt-5.6-sol`.
- Harness: `buzz-acp` with `BUZZ_ACP_AGENT_COMMAND=hermes`,
  `BUZZ_ACP_AGENT_ARGS=-p,crewspike,acp`,
  `BUZZ_ACP_MCP_COMMAND=<repo>/target/release/buzz-dev-mcp`,
  `respond_to=owner-only`.
- Startup line confirms the exact spawn:
  `agent_cmd=hermes -p crewspike acp`, `model=(agent default)` (no
  `BUZZ_ACP_MODEL` anywhere — C-05 manual reading).
- Adapter log confirms profile binding:
  `Loaded env from /Users/…/.hermes/profiles/crewspike/.env`.

## C-02 — mention → profile-bound turn → signed agent reply: PASS

1. Owner posted a kind-9 mention asking the agent to post exactly
   `CREW-LIVE-OK`.
2. Harness dispatched the turn; Hermes ran on the profile's model
   (`model=gpt-5.6-sol` in every API-call log line).
3. Agent posted `CREW-LIVE-OK` into the channel via the
   `mcp__buzz_dev_mcp__shell` tool (`buzz messages send …`).
4. Channel read-back over the relay shows the standalone message
   `CREW-LIVE-OK` authored by the agent pubkey `343ae0ef…`.

## C-07 (strict) — profile model change picked up without respawn: PASS

1. With the same harness + adapter process still running (uptime
   continuous, no respawn), changed the profile:
   `hermes -p crewspike config set model.default gpt-5.6-terra`.
2. Owner sent the `!rotate` control command (session invalidation), then a
   new mention.
3. The next turn — same adapter process — logged
   `model=gpt-5.6-terra provider=openai-codex` for every API call.

No Crew-side edit, no harness restart, no restart badge. The
`!rotate`-then-next-mention path is the documented manual trigger; natural
session rotation reaches the same code path.

## New finding — Hermes strips `BUZZ_*` from its own terminal tool (feeds §7.2/§9)

First attempts had the agent try `buzz messages send` through Hermes'
native `terminal` tool and fail:
`auth error: BUZZ_PRIVATE_KEY is required` (exit 3). Root cause, verified
in Hermes source: `BUZZ_PRIVATE_KEY`, `BUZZ_RELAY_URL`, `BUZZ_AUTH_TAG`
(and other `BUZZ_*`) are in `_HERMES_PROVIDER_ENV_BLOCKLIST`
(`tools/environments/local.py`), and the `terminal.env_passthrough`
config path refuses blocklisted names by design (GHSA-rhgp-j443-p4rf
mirror filter). Hermes' sandbox treats relay credentials as protected
secrets — good security default, but it means:

- **A Hermes agent can only reply through the harness-provided MCP server**
  (`buzz-dev-mcp`), which receives `BUZZ_*` via ACP `session/new`
  `mcpServers[].env`, outside the sandbox scrub.
- `BUZZ_ACP_MCP_COMMAND` is therefore **mandatory** for Hermes agents.
  Desktop consequence: tier-3 custom harness JSON cannot declare an MCP
  command, so each Hermes agent needs env var
  `BUZZ_ACP_MCP_COMMAND=<path-to-buzz-dev-mcp>` until the upstream tier-1
  entry lands with `mcp_command: Some("buzz-dev-mcp")` (same shape as
  Codex, whose entry already does this — `discovery.rs:144`).
- The upstream tier-1 entry proposal in the feature doc §7.2 is updated
  accordingly.

## Cleanup

- Killed harness + relay processes; stopped the remapped Postgres/Redis
  containers; deleted profile `crewspike` (verified directory absence);
  removed temp compose/key/output files. The `hermes-probe`/`hermes-live`
  channels remain in the throwaway local relay DB only.

## Limits

- One machine, macOS arm64, Hermes v0.20.0, one agent, short turns.
- `respond_to=owner-only` path only; steering/interrupt untested here.
- Desktop-app spawn path not exercised (harness run manually with the
  same env the desktop would set); Slice 2 E2E covers the UI path.
