# Hermes agents in Crew — runbook

- **Feature:** [`features/0001-hermes-first-class-runtime.md`](features/0001-hermes-first-class-runtime.md)
- **Decision:** [`DECISIONS.md`](DECISIONS.md) D-019, D-020, D-023
- **Status of this flow:** tier-1 runtime on main; Phase 02 binding UI on
  main; Phase 03 profile lifecycle (create-in-place, keep/delete
  offboarding, orphan repair) on main.

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
   `model.default …`) — never in Crew. The create/edit UI shows
   "Model: decided by profile \<name\>" and does not offer an editable
   model control. Crew also strips `BUZZ_ACP_MODEL` at spawn for
   profile-locked runtimes (Phase 02A).
3. Spawn shape is `hermes` with args `-p <profile> acp`. The desktop
   injects `-p <bound name>` from `ManagedAgentRecord.hermes_profile`
   when the catalog entry has `profile_arg`. Never use a renamed
   wrapper binary — `buzz-acp` keys per-runtime defaults (e.g.
   `HERMES_ACP_SKIP_CONFIGURED_MCP=1`) off the command basename.
4. `parallelism` stays `1` for Hermes agents (spike 0012).
5. Public agents (`respond-to` ≠ owner-only) require the
   credential-isolation caveat below.

## Hiring a new agent

Example: an agent called `scout`.

### 1. Create the profile (CLI or create-in-place)

**Preferred (Phase 04):** in Crew, pick runtime **Hermes Agent**, open
the **Hermes profile** control, and **pick an existing profile** from
disk (`scout`, `builder`, …) or type a new name and click
**Create profile '\<name\>'**. Crew lists `~/.hermes/profiles` via
`list_hermes_profiles`. Create runs
`hermes profile create <name> --no-alias` (command line shown in the
UI) and binds on success. Bundled skills are kept (D-023); there is no
`--no-skills` from Crew. Profiles already bound on this relay show a
**bound** badge; save is blocked client-side (server C-10 still applies).

**CLI fallback:**

```bash
hermes profile create scout --no-alias --description "Research agent for Crew"
hermes -p scout config set model.provider <provider>
hermes -p scout config set model.default <model-id>
```

Names must match `[a-z0-9][a-z0-9_-]{0,63}`. `--no-alias` because Crew
binds by name, not by wrapper script. Add `--no-skills` only when you
want an empty profile from the CLI (Crew does not offer this).

### 2. Create / bind the Crew agent

Select the profile from the list or keep the name you just created
(`scout`). Leave model blank — the UI replaces the model control with
"decided by profile scout". Binding `default` is rejected (client and
server). Binding a profile already used by another agent on the same
relay shows an occupancy error and disables save; the server still
rejects duplicates (C-10) if forced.

Readiness / Doctor surfaces a `hermesProfile` requirement when the
binding is missing, and a recreate/rebind repair when the bound
profile directory is absent (orphan).

### Legacy: tier-3 custom harness JSON (optional)

Per-profile JSON under
`~/Library/Application Support/com.nuncio.crew/custom_harnesses/` that
baked `-p <profile>` into `args` is **legacy**. Prefer the binding
field on the builtin Hermes runtime. Existing JSONs still work if you
need them; new agents should not add more.

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

Deleting a Hermes agent in Crew asks:

- **Keep profile '\<name\>' (memory + skills)** — default. Record gone;
  profile intact and re-attachable (C-13).
- **Also delete the profile** — runs
  `hermes profile delete <name> -y` and verifies by directory absence
  (C-14 / spike 0011). Never preselected.

**CLI fallback** (if you deleted the Crew record already and kept the
profile, or need to clean up outside Crew):

```bash
hermes profile delete scout -y
```

Always pass `-y`: on a non-TTY, a bare `delete` auto-cancels **with exit
code 0** (spike 0011) — verify by directory absence, not exit code.

## Failure classes (C-03/C-12)

| Symptom | Cause | Fix |
| ------- | ----- | --- |
| Runtime shows unavailable / MissingBinary | `hermes` not on PATH for the desktop app | Install Hermes; check PATH the app sees |
| Config nudge: bind Hermes profile | No `hermes_profile` on the record | Edit Agent → Hermes profile; or create-in-place |
| Config nudge: profile missing on disk | Profile deleted/renamed outside Crew | Recreate profile / Change binding in the nudge |
| Spawn exits immediately, log shows `Profile 'x' does not exist…` | Same orphan class | Same repair path |
| Agent replies with `auth error: BUZZ_PRIVATE_KEY is required` | Reply path missing `buzz-dev-mcp` (Hermes sandbox strips `BUZZ_*`) | Use the tier-1 **Hermes Agent** runtime (attaches MCP automatically); ensure `buzz-dev-mcp` is on PATH the app sees |
| Agent replies with `model: String should have at least 1 character` | Profile has no model configured | `hermes -p <name> config set model.default …` |
| Agent replies with a provider billing/auth error | Profile's provider unauthenticated or out of credit | `hermes -p <name> …` auth flow for that provider |
| Save error: profile already bound | Another agent on this relay uses that profile (C-10) | Pick a different profile name |

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
  a Hermes agent bound to a fallback-enabled profile. Crew shows a
  one-line warning on create-in-place and offboarding when respond-to is
  not owner-only.
- Fresh profiles contain no gateway config and no cron jobs — personal
  messaging surfaces stay out of agent profiles as long as you never
  bind `~/.hermes` itself.

## Known gaps

- UI binding field + profile-owned model display — **done (Phase 02B)**.
- `BUZZ_ACP_MODEL` spawn guard + duplicate-bind reject — **done (Phase 02A)**.
- Profile listing / create-from-UI lifecycle + keep/delete offboarding +
  orphan repair — **done (Phase 03)**.
- Auth badge — blocked on Hermes-side probe (spike 0010 / feature §7.3).
- Live session model in the "decided by profile" row — optional follow-up
  when a clean ACP session-catalog read path exists from create/edit.
- Credential isolation for public agents — blocked on Hermes-side ask.
