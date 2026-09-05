# Hermes agents in Crew — runbook

- **Feature:** [`features/0001-hermes-first-class-runtime.md`](features/0001-hermes-first-class-runtime.md)
- **Decision:** [`DECISIONS.md`](DECISIONS.md) D-019, D-020, D-023, D-024, D-025
- **Product direction:** [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md) (Hermes-first,
  still on Buzz contracts — do not invent a parallel Hermes protocol)
- **Status of this flow:** tier-1 runtime on main; Phase 02 binding UI on
  main; Phase 03 profile lifecycle (create-in-place, keep/delete
  offboarding, orphan repair) on main. Issue #104 Phase 01 adds the trusted
  owner/local boundary and multi-community shared-state visibility.

## The model in one sentence

**An agent is a Hermes profile; Crew is the office it works in.** The
profile (`~/.hermes/profiles/<name>`) owns the agent's model, provider,
memory, skills, and credentials. Crew owns identity-on-relay, channel
placement, scheduling, and display. Room assignment and results still use
**Buzz** (Nostr + ACP); Hermes is the richest employee implementation, not
a second backend.

## Rules (D-019 + D-024 summary)

1. One local Crew managed-agent record ↔ one Hermes profile. That record
   owns runtime pairs in every configured community, intentionally sharing
   memory, skills, and profile-owned state across them. A second local record
   cannot bind the same profile. Binding the manager's default profile
   (`~/.hermes`) requires explicit confirmation; Crew still does not edit,
   archive, or delete that home profile.
2. Model and provider remain profile-owned, but Crew is their write-through
   editor for **named** profiles. The create/edit UI reads `config.yaml` and
   saves changed values only through `hermes -p <name> config set
   model.provider …` and `model.default …`; Crew never stores a competing
   copy. The same UI edits the profile's `SOUL.md`, shared everywhere that
   profile runs. Both changes apply on the next fresh ACP session; `!rotate`
   forces one. Crew strips `BUZZ_ACP_MODEL` at spawn for profile-locked
   runtimes. Bound `default` is read-only in Crew: edit that profile in
   Hermes.
3. Spawn shape is `hermes` with args `-p <profile> acp`, including the home
   profile (`hermes -p default acp` — spike 0056). The desktop injects
   `-p <bound name>` from `ManagedAgentRecord.hermes_profile` when the
   catalog entry has `profile_arg`. Never use a renamed
   wrapper binary — `buzz-acp` keys per-runtime defaults (e.g.
   `HERMES_ACP_SKIP_CONFIGURED_MCP=1`) off the command basename.
4. `parallelism` stays `1` for Hermes agents (spike 0012).
5. A profile-bound Hermes agent is always `owner-only` and local. Backend
   validation rejects `allowlist`/`anyone` and provider/remote deployment on
   create, update, start, and deploy; client-side controls are explanatory,
   not the authority.
6. Full autonomy is intentional for this trusted owner-operated boundary.
   In the default `bypass-permissions` harness mode, ACP requests select a
   valid advertised `allow_once`; Crew has no
   dangerous-command permission inbox. Clarification/elicitation requests are
   separate and may enter **Needs You**. Hermes' profile-owned
   `approvals.mode` still applies; Crew does not override it.

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
`--no-skills` from Crew. Profiles already bound to another local managed-agent
record show a **bound** badge; save is blocked client-side (server C-10 still
applies).

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
(`scout`). After create succeeds, Crew offers a skippable persona-at-birth
step for the new profile's `SOUL.md`; skipping preserves Hermes' generated
default. The model/provider editor reads the profile and writes through the
Hermes CLI, with a note that changes affect every agent using the profile.
Binding `default` is offered as **Personal (default)** and requires an
explicit confirmation that names the shared surfaces. Cancel leaves the
field unbound. Crew still will not write SOUL.md or `config.yaml` under
`~/.hermes`. Binding a profile already used by another local managed-agent record
shows an occupancy error and disables save; the server still rejects duplicates
(C-10) if forced. One managed-agent record owns runtime pairs for all
configured communities, and the Phase 01 UI says that memory, skills, and
profile state are shared across that reach.

The effective boundary shown during create/edit is:

```text
Access       Owner only
Autonomy     Full
Backend      This Mac
Profile      scout
```

Selecting public access or a remote backend must block save with actionable
copy; a warning-only path is not sufficient.

Readiness / Doctor surfaces a `hermesProfile` requirement when the
binding is missing, and a recreate/rebind repair when the bound
profile directory is absent (orphan).

There is no "Reset to Hermes default" action: Hermes is the only trustworthy
source for that generated default, so Crew does not ship a copy that could
drift.

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
- **Permissions:** no operator action is expected. `buzz-acp` deliberately
  answers Hermes ACP permission requests with a valid advertised `allow_once`
  option under the default `bypass-permissions` harness mode. Other modes
  reject or cancel; persistent `allow_always` grants are never selected. See
  [ACP permission choices](../../crates/buzz-acp/README.md#turn-deadlines-and-permission-choices). Do not route these requests into Mission Inbox. A genuine Hermes
  clarification is a different protocol flow and may become a **Needs You**
  item. The profile's Hermes approval policy remains authoritative. When the
  owner intentionally wants total profile-level bypass, use Hermes' canonical
  surface (`hermes -p scout config set approvals.mode off`); Crew never stores
  or mutates that policy.
- **Reuse across communities:** the one managed agent bound to `scout` owns a
  runtime pair in each configured community. Changes to memory, skills,
  credentials, and other profile-owned state affect every one of those pairs.

## Offboarding

Deleting a Hermes agent in Crew asks (D-035):

- **Keep profile '\<name\>' (memory + skills)** — default. Record gone;
  profile intact and re-attachable (C-13).
- **Archive the profile** — never preselected. The profile is packed to
  `<nest>/profile-archives/<profile>-<timestamp>.tar.gz` with a sidecar
  manifest, then removed from `~/.hermes/profiles/`. Memory and skills are
  preserved; caches (`audio_cache/`, `image_cache/`, `logs/`) are excluded, and
  the dialog shows the estimated size before you confirm. An optional reason is
  stored in the manifest.

Archiving is copy → verify → remove: if anything fails, the live profile is
still there. Crew refuses to archive while a runtime pair bound to that profile
is running — stop the agent first; Crew will not kill it for you.

**Restore** lists archives with their manifest facts, unpacks to
`~/.hermes/profiles/<name>`, and offers to re-bind the original agent. A name
collision with a live profile blocks the restore and changes nothing.

**Permanent delete** exists only as an action on an archive, and only after you
type the profile name exactly. It destroys those memories and skills for good.

There is no `hermes` CLI equivalent for the archive: the archive area is
Crew-owned. The Hermes CLI can still delete a profile outright
(`hermes profile delete scout -y`) — that is irreversible and bypasses the
archive. Always pass `-y`: on a non-TTY, a bare `delete` auto-cancels **with
exit code 0** (spike 0011) — verify by directory absence, not exit code.

## Profile readiness

Crew classifies a bound profile into one named state (issue #119) rather than a
healthy/orphaned boolean. The state shows on the agent card in Agents and on
the agent's config surfaces, and is recomputed on every status read, so
breakage appears — and repairs clear — without restarting the app.

| State | Meaning | Blocks start |
| ----- | ------- | ------------ |
| `ready` | Reserved for a truthful Hermes auth probe; not reachable today | no |
| `missing` | Bound profile directory is gone | yes |
| `broken-config` | `config.yaml` exists but does not parse | yes |
| `binary-missing` | Resolved `hermes` command will not run | yes |
| `auth-unknown` | Healthy on disk; **auth not verifiable** (spike 0010) | no |

Blocking states ride the normal requirement pipeline, so a start is diverted
into the existing setup/config-nudge flow with a named repair row instead of
stalling mid-turn (C-03). `auth-unknown` is advisory only: it never blocks a
start and never claims auth is good. Crew does not scrape `auth.json` and does
not make test API calls to fake a green badge.

## Failure classes (C-03/C-12)

| Symptom | Cause | Fix |
| ------- | ----- | --- |
| Runtime shows unavailable / MissingBinary | `hermes` not on PATH for the desktop app | Install Hermes; check PATH the app sees |
| Config nudge: bind Hermes profile | No `hermes_profile` on the record | Edit Agent → Hermes profile; or create-in-place |
| Config nudge: profile missing on disk | Profile deleted/renamed outside Crew | Recreate profile / Change binding in the nudge / restore an archive |
| Card reads `Config invalid` | Profile `config.yaml` does not parse | Fix the YAML the diagnostic names, then start again |
| Card reads `Auth not verifiable` | Expected: no Hermes headless auth probe (spike 0010) | Nothing to fix; auth problems still surface in-channel |
| Spawn exits immediately, log shows `Profile 'x' does not exist…` | Same orphan class | Same repair path |
| Agent replies with `auth error: BUZZ_PRIVATE_KEY is required` | Reply path missing `buzz-dev-mcp` (Hermes sandbox strips `BUZZ_*`) | Use the tier-1 **Hermes Agent** runtime (attaches MCP automatically); ensure `buzz-dev-mcp` is on PATH the app sees |
| Agent replies with `model: String should have at least 1 character` | Profile has no model configured | `hermes -p <name> config set model.default …` |
| Agent replies with a provider billing/auth error | Profile's provider unauthenticated or out of credit | `hermes -p <name> …` auth flow for that provider |
| Save error: profile already bound | Another local managed-agent record uses that profile (C-10) | Reuse that agent or pick a different profile name |
| Save error: profile-bound agent must run locally | A legacy record is pinned to a remote backend | Stop/delete the Crew record, keep the Hermes profile, and recreate the agent on **This computer**. The legacy record remains readable, stoppable, and deletable. |

There is currently **no headless auth probe** (spike 0010): Hermes
`auth status` always exits 0, so Crew cannot badge auth state. Auth
problems surface reactively as in-channel errors — by design until the
Hermes-side ask lands.

## Security caveats

- **Credential fallback (spike 0010):** a fresh profile stores no
  credentials of its own but *reads the manager's pooled credentials*
  through a global-root fallback. A Hermes agent in Crew can therefore
  spend the manager's provider credit. This is acceptable only inside D-024's
  trusted one-manager boundary. Profile-bound Hermes agents are `owner-only`;
  public or allowlisted access is rejected rather than downgraded to a
  warning.
- **Local profile custody:** a profile name points at state on this machine.
  Provider/remote backends are rejected until a separate secure profile and
  secret provisioning design exists.
- **Shared profile state:** multi-community reuse is deliberate but not
  isolated. Memory, skills, credentials, and other profile-owned state are
  shared by one installation-wide managed-agent record. A second local record
  cannot bind the same profile; this prevents duplicate workers while leaving
  cross-installation provisioning outside Crew's scope.
- Fresh profiles contain no gateway config and no cron jobs — personal
  messaging surfaces stay out of agent profiles as long as you never
  bind `~/.hermes` itself.

## Known gaps

- UI binding field + profile-owned model display — **done (Phase 02B)**.
- `BUZZ_ACP_MODEL` spawn guard + duplicate-bind reject — **done (Phase 02A)**.
- Profile listing / create-from-UI lifecycle + keep/delete offboarding +
  orphan repair — **done (Phase 03)**.
- Owner-only/local backend enforcement and multi-community shared-state copy —
  **done (Issue #104 Phase 01)**.
- Auth badge — blocked on Hermes-side probe (spike 0010 / feature §7.3);
  `auth-unknown` is the honest placeholder (D-035, issue #119).
- `broken-config` detects unparsable `config.yaml` only; missing/invalid model
  fields are not classified locally (spike 0015).
- Live session model in the "decided by profile" row — optional follow-up
  when a clean ACP session-catalog read path exists from create/edit.
- Credential isolation for public agents — blocked on Hermes-side ask.

## Huddle voice latency levers (issue #275, upstream #5671)

Upstream's huddle latency work is **env-gated and default-off**, and Crew has
absorbed only its always-on parts. Crew's huddle voice path therefore has **no
tuning environment variables** today — nothing to set on an agent profile, in
`.env`, or in a managed-agent env block.

Absorbed (always-on, no configuration):

- A held push-to-talk shortcut groups the whole hold into one utterance; a VAD
  pause no longer splits it even with a manually open microphone
  (`vad_flush_allowed` in `desktop/src-tauri/src/huddle/stt.rs`).
- Speech-boundary policy: onset confirmation, pre-roll, offset hysteresis and
  hangover (upstream #6397), so short replies survive and trailing consonants
  are not clipped.

Not absorbed here — these levers live in `buzz-voice`/TTS internals that this
branch does not touch, so **setting them has no effect in Crew**:
`BUZZ_STT_SPECULATIVE`, `BUZZ_TTS_STREAMING`, `BUZZ_TTS_EMIT_FRAMES`,
`BUZZ_STT_THREADS`, `BUZZ_TTS_THREADS`, `BUZZ_TTS_PHASE_LOG`.

`BUZZ_STT_FLUSH_MS` is **not** a latency lever and must not be reintroduced:
upstream removed it after live testing because shortening the silence window
below natural mid-sentence pauses split one spoken sentence into several
messages and confused listening agents. The window stays at the production
300 ms (`SILENCE_FLUSH_FRAMES`).

## Crew roles (issue #116 Slice 1R and later slices)

- **Model:** a role is a founder/owner-signed `(agent, channel)` assignment
  carried in that channel's fenced `crew` canvas block. Labels are free-form
  validated strings; the founder-authored definition travels with the
  assignment. There is no global taxonomy.
- **Authority:** agents cannot self-assign. On read, the harness accepts the
  Crew block only when the canvas author matches the agent's owner/founder
  pubkey; non-owner blocks are ignored (D-028, D-043).
- **Prompt:** buzz-acp injects a role section (definition text,
  allowed/not-allowed/refuse-and-redirect guidance, and the mandatory first
  reply `ROLE-CHECK`) on each **fresh channel session** when a valid assignment
  is present. No assignment means no role section.
- **Change semantics:** the owner updates the channel canvas. The next fresh
  session for that channel reads the latest replaceable canvas event; no agent
  respawn is required. Concurrent canvas edits use replaceable-event LWW
  semantics.
- **Routing and capabilities:** routing presets and founder-authored capability
  keys are additional fields in the same channel Crew block. `buzz-dev-mcp`
  denial is enforced through the per-session MCP list where that is the
  engine's tool boundary, and on engines with their own file/shell tools the
  same denial also clamps that session to a read-only native mode addressed by
  session ID (Codex `read-only`, Grok `plan`; see D-044 amendment 2). Where an
  engine refuses the session-scoped floor, the harness says so through
  `session_capability_floor.enforcement: "advisory"` and the denial is a Crew
  rule rather than a wall for that engine only. Hermes profile ownership remains a boundary for profile
  memory/skills/credentials/model, not the capability boundary (D-024,
  D-029, D-044).

## Call by name (issue #230)

When another agent is needed, **call them by name** — e.g.
`buzz agents call --channel <UUID> --agent Dev` (optional `--reply-to`).
That posts a room-visible mention so a sleeping same-owner sibling wakes on
the existing ACP path (#169). Do **not** call by role label. Do **not** ask
the founder to press Wake. Channel roles still decide what that named person
may do once awake (D-043 / D-044). See D-071 / spike 0055.

## CoS intake (issue #232)

On an office channel, assign CoS the **intake** role and Dev the **code**
role (canvas; D-043 / D-044). Oscar @CoS only. CoS may prototype small;
feature work → call Dev by name (#230 / D-071). Gate C before Accept
(D-070). Template: [`templates/COS-INTAKE.md`](templates/COS-INTAKE.md).
See D-072.

## Officer loop — removed with Org product (#233 / D-069)

Do **not** run ORG-CHECK, teach the org chart, or route work through a
manager tree. Channel roles (D-043 / D-044) say what a named person may
do in the room. CoS calls specialists **by name** (#230 / #232). Peer
chat stays flat. `KIND_ORG_ROSTER` may still exist on the relay for sync;
it is not how Crew companies run.
