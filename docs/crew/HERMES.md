# Hermes agents in Crew — Slice 1 runbook

- **Feature:** [`features/0001-hermes-first-class-runtime.md`](features/0001-hermes-first-class-runtime.md)
- **Decision:** [`DECISIONS.md`](DECISIONS.md) D-019
- **Status of this flow:** tier-1 runtime on main (PR #54); profile
  binding is still manual until Phase 02. Conventions below are
  enforced by this document until the UI lands.

## The model in one sentence

**An agent is a Hermes profile; Crew is the office it works in.** The
profile (`~/.hermes/profiles/<name>`) owns the agent's model, provider,
memory, skills, and credentials. Crew owns identity-on-relay, channel
placement, scheduling, and display.

## Rules (D-019 summary)

1. One Crew agent ↔ one named Hermes profile. Never share a profile
   between two live agents; never bind the manager's default profile
   (`~/.hermes`).
2. Model and provider are configured on the profile
   (`hermes -p <name> config set model.provider …` /
   `model.default …`) — never in Crew. Leave persona/agent model fields
   blank; do not set `BUZZ_ACP_MODEL` anywhere for Hermes agents.
3. Spawn shape is `hermes` with args `-p <profile> acp`. Never use a
   renamed wrapper binary — `buzz-acp` keys per-runtime defaults (e.g.
   `HERMES_ACP_SKIP_CONFIGURED_MCP=1`) off the command basename.
4. `parallelism` stays `1` for Hermes agents (spike 0012).
5. Public agents (`respond-to` ≠ owner-only) require the
   credential-isolation caveat below.

## Hiring a new agent (manual flow)

Example: an agent called `scout`.

### 1. Create the profile

```bash
hermes profile create scout --no-alias --description "Research agent for Crew"
hermes -p scout config set model.provider <provider>
hermes -p scout config set model.default <model-id>
```

Names must match `[a-z0-9][a-z0-9_-]{0,63}`. `--no-alias` because Crew
binds by name, not by wrapper script. Add `--no-skills` for an empty
profile (default bundles ~70 skills).

### 2. Register the harness (tier-3 custom harness JSON)

Create
`~/Library/Application Support/com.nuncio.crew/custom_harnesses/hermes-scout.json`:

```json
{
  "id": "hermes-scout",
  "label": "Hermes (scout)",
  "command": "hermes",
  "args": ["-p", "scout", "acp"],
  "installInstructionsUrl": "https://hermes-agent.nousresearch.com",
  "installHint": "Runs Hermes bound to the 'scout' profile. Create it first: hermes profile create scout --no-alias."
}
```

One file per profile. The id must not be a reserved builtin id (`hermes`
itself is reserved); `hermes-<profile>` is the convention. Restart the
desktop app (or reopen Settings → runtimes) to re-run discovery.

With the tier-1 `Hermes Agent` runtime on main (PR #54), the per-profile
JSON's only remaining job is carrying the `-p <profile>` args. You can
instead pick runtime **Hermes Agent** and set agent args
`-p,<profile>,acp` directly. Phase 02 replaces both with a proper
binding field.

### 3. Create the Crew agent

In Crew, create the agent with runtime `Hermes (scout)` (or **Hermes
Agent** plus `-p,<profile>,acp` args — see above). Leave model and
provider blank everywhere (agent record, persona, global default — and
never add `BUZZ_ACP_MODEL` to any env-var map; Phase 02 automates this
guard, until then it is manual discipline).

The tier-1 Hermes runtime attaches `buzz-dev-mcp` automatically at spawn
(`mcp_command` in `known_runtimes.rs`). That MCP path is required because
Hermes' terminal sandbox strips `BUZZ_*` credentials — see verification
0006.

## Daily operations

- **Change the model:** `hermes -p scout config set model.default <id>` —
  takes effect on the agent's next fresh ACP session. No Crew edit or
  restart badge. Send `!rotate` (as the agent owner, mentioning the
  agent) to force a fresh session immediately — verified live in
  verification 0006: the next turn ran on the new model with no respawn.
- **Teach a skill / add memory:** work with the profile directly
  (`hermes -p scout …`); Crew inherits automatically on the next turn.
- **Inspect what the agent knows:** `hermes -p scout` surfaces (sessions,
  memory, skills) — Crew holds none of it.

## Offboarding

```bash
# keep the profile (re-hire later): just delete the Crew agent record.
# delete everything:
hermes profile delete scout -y
```

Always pass `-y`: on a non-TTY, a bare `delete` auto-cancels **with exit
code 0** (spike 0011) — verify by directory absence, not exit code. Then
remove the `hermes-scout.json` harness file.

## Failure classes (C-03/C-12, current manual reading)

| Symptom | Cause | Fix |
| ------- | ----- | --- |
| Runtime shows unavailable | `hermes` not on PATH for the desktop app | Install Hermes; check PATH the app sees |
| Spawn exits immediately, log shows `Profile 'x' does not exist. Create it with: hermes profile create x` | Profile deleted/renamed outside Crew | Recreate the profile or rebind the agent |
| Agent replies with `auth error: BUZZ_PRIVATE_KEY is required` | Reply path missing `buzz-dev-mcp` (Hermes sandbox strips `BUZZ_*`) | Use the tier-1 **Hermes Agent** runtime (attaches MCP automatically); ensure `buzz-dev-mcp` is on PATH the app sees |
| Agent replies with `model: String should have at least 1 character` | Profile has no model configured | `hermes -p <name> config set model.default …` |
| Agent replies with a provider billing/auth error | Profile's provider unauthenticated or out of credit | `hermes -p <name> …` auth flow for that provider |

There is currently **no headless auth probe** (spike 0010): Hermes
`auth status` always exits 0, so Crew cannot badge auth state. Auth
problems surface reactively as in-channel errors — by design until the
Hermes-side ask lands.

## Security caveats

- **Credential fallback (spike 0010):** a fresh profile stores no
  credentials of its own but *reads the manager's pooled credentials*
  through a global-root fallback. A Hermes agent in Crew can therefore
  spend the manager's provider credit. Acceptable for owner-only agents
  on a one-manager machine; **not acceptable for public agents** until a
  per-profile isolation switch exists. Do not set `respond-to anyone` on
  a Hermes agent bound to a fallback-enabled profile.
- Fresh profiles contain no gateway config and no cron jobs — personal
  messaging surfaces stay out of agent profiles as long as you never
  bind `~/.hermes` itself.

## Known gaps in this slice (by design)

- No UI for binding/creating profiles (Phase 02 / Phase 03).
- No `BUZZ_ACP_MODEL` guard yet (Phase 02; spike 0013).
- No auth badge (blocked on Hermes-side probe, §7.3 of the feature doc).
