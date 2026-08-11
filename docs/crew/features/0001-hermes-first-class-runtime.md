# Feature 0001 — Hermes as a first-class Crew runtime (profile-per-agent)

- **Status:** PROPOSED — Slice 0 spikes complete (see §9); awaiting manager
  approval for Slice 1
- **Date:** 2026-08-04 (spikes completed 2026-08-05)
- **Driver:** The manager is moving the whole Crew agent workflow onto the
  Hermes engine and wants it done correctly from the start, before any Hermes
  agent exists in Crew.
- **Workflow position:** This document is the feature plan. Per
  [`README.md`](../README.md) and D-008, no production implementation begins
  until the Slice 0 spikes are conclusive, contract tests are RED, and the
  manager approves the resulting plan.

---

## 1. Summary

Crew currently treats Hermes as a tier-2 preset harness: a name, a binary
probe, and a docs link. This feature promotes Hermes to the primary, fully
supported runtime for Crew agents, built on one organizing principle:

> **An agent is a Hermes profile. Crew is the office it works in.**

Each Crew agent binds 1:1 to a Hermes profile (`~/.hermes/profiles/<name>`),
which owns the agent's model, provider, memory, skills, and credentials. Crew
owns placement (channels, mentions, board, worktrees) and display. Model
selection is removed from Crew's UI for Hermes agents — the profile is the
single source of truth.

The work is split across three owners to respect the thin-fork rules:

| Owner | What it gets |
| ----- | ------------ |
| Crew (this fork) | Profile binding, profile-aware create/delete flows, readiness surface, model-field suppression |
| Upstream `block/buzz` | Tier-1 `KnownAcpRuntime` entry for Hermes (PR upstream, then received via sync — zero permanent fork drift) |
| Hermes (`nousresearch`) | Headless auth/readiness probe with exit-code semantics; non-interactive profile provisioning guarantees |

---

## 2. Glossary

| Term | Meaning |
| ---- | ------- |
| Hermes | The Hermes Agent CLI (`hermes` on PATH), an ACP-speaking agent runtime by Nous Research |
| Profile | A self-contained Hermes home (`~/.hermes/profiles/<name>`): model + provider config, memories, skills, plugins, cron, credentials, session history |
| Tier-1 runtime | A compiled-in `KnownAcpRuntime` entry in Buzz desktop with auth probes, readiness requirements, default env, and first-class onboarding |
| Tier-2 preset | A static `PresetHarness` entry: PATH-probed, docs link only, no auth/readiness intelligence |
| Harness | `buzz-acp`, the process that bridges relay events to an ACP agent subprocess |
| Persona / definition | Buzz's reusable agent template (system prompt, model, provider, runtime, env) |

---

## 3. User stories

The manager persona below is "Oscar", a single operator running Crew as
mission control (per [`VISION.md`](../VISION.md)). Two initial agents:
**Scout** (research, fast/cheap model) and **Builder** (long-running coding,
strong model). Each story carries acceptance criteria (AC) that later become
contract tests.

### Epic 1 — First contact

**S-1.1 — Hermes stands as an equal.**
As Oscar, when I open the runtime picker, I see Hermes alongside
Goose/Claude/Codex with a logo and a truthful status, so I trust it is a
supported path rather than a hack.

- AC1: With only the `hermes` binary installed (no `hermes-acp` shim), the
  catalog reports Hermes as installed/available.
- AC2: With Hermes absent, the catalog shows install guidance, not a silent
  failure.

**S-1.2 — Know what is missing before starting.**
As Oscar, if Hermes is not ready (not installed, provider not authenticated),
Crew tells me exactly what is missing and how to fix it *before* I create the
agent — not via a dead agent and a log dive afterwards.

- AC1: An unauthenticated profile surfaces as "needs login" with a concrete
  command hint, at or before agent creation.
- AC2: A spawn is never attempted against a profile known to be unready;
  the block is explained in UI copy.

### Epic 2 — Hiring an agent

**S-2.1 — Every agent gets its own laptop.**
As Oscar, when I create agent "Scout", Crew asks which Hermes profile Scout
uses — pick an existing one or create one in place — so Scout has its own
memory, skills, and credentials, sharing nothing by accident.

- AC1: Agent creation with runtime=Hermes requires a profile binding; there
  is no unbound default that silently lands on `~/.hermes` (the manager's
  personal default profile).
- AC2: Creating a profile from Crew is an explicit, visible action (never a
  hidden shell side effect), and a failed creation aborts agent creation with
  a clear error.
- AC3: The manager's personal default profile is never selectable for a
  Crew agent unless explicitly confirmed (it holds personal gateways,
  cron jobs, and credentials).

**S-2.2 — No model question (anti-story).**
As Oscar, when creating or editing a Hermes agent, I never see a model
picker. I see read-only information: "Brain: decided by profile *scout* —
currently `<model>`".

- AC1: Persona/agent editor renders no editable model/provider field for
  runtime=Hermes.
- AC2: The spawn environment contains no `BUZZ_ACP_MODEL` for Hermes agents
  — from any layer (record, persona, global default, or env-var maps).
- AC3: If a stale/global model value would have leaked, the effective spawn
  still carries none (enforced, not just documented).

**S-2.3 — Same procedure, different personality.**
As Oscar, creating Builder is the same two-minute flow as Scout — only the
bound profile differs. Hiring is repeatable, not a configuration project.

- AC1: Second agent creation reuses the identical flow with a different
  profile and produces an isolated, working agent.

### Epic 3 — Daily work

**S-3.1 — Assign by mention; the agent remembers only its own life.**
As Oscar, when I `@Scout` in #research, Scout answers with the memory it
accumulated in its own past work — and can never recall conversations or use
skills that belong to Builder.

- AC1: Two Hermes agents in the same community observe disjoint memory and
  skill stores (probe: teach agent A a fact/skill; agent B cannot surface it).
- AC2: Concurrent turns by both agents produce no cross-profile state
  corruption.

**S-3.2 — Upgrade the brain without touching Crew.**
As Oscar, when I change the model on the Builder profile (one command,
outside Crew), the next assigned task uses the new model. No Crew restart, no
second field to update, no restart badge.

- AC1: A profile-side model change is reflected in the agent's next session
  without any Crew edit or agent respawn.
- AC2: Crew's display of "current model" follows the profile's catalog
  advertisement (ACP `session/new` models payload), not a Crew-stored value.

**S-3.3 — Teach once, works everywhere.**
As Oscar, a skill I teach Scout directly (terminal or another Hermes surface)
is available when Scout works inside Crew — one employee, many offices.

- AC1: A skill installed into the Scout profile outside Crew is usable in a
  Crew-dispatched turn without Crew-side action.

### Epic 4 — A fuller office

**S-4.1 — Three agents in `Working`, no shared notebook.**
As Oscar, when the board has three cards in Working and three Hermes agents
run in parallel, no two processes fight over one profile's session store.

- AC1: N concurrent Crew agents = N distinct profiles by construction; the
  UI resists (warns or blocks) binding two live agents to one profile.
- AC2: A single agent record with `parallelism > 1` on a Hermes runtime is
  validated against profile-store concurrency limits (spike S0-4 decides:
  cap to 1, or prove the store is safe).

**S-4.2 — A public agent does not hold my house keys.**
As Oscar, if I open an agent to anyone in a channel (`respond-to anyone`),
that agent's profile contains only what I provisioned for it — never my
personal messaging gateways, cron jobs, or credentials.

- AC1: Crew-created profiles start without their own gateway config, cron
  jobs, or stored secrets — **but spike 0010 proved the credential pool
  falls back read-only to the manager's global root store**, so a fresh
  profile can still *use* the manager's provider credentials. Credential
  isolation for public agents therefore requires an explicit provisioning
  step (documented in Slice 1; mechanism ask filed with Hermes in §7.3).
- AC2: Docs and UI copy state the boundary explicitly for public agents,
  including the credential-fallback caveat.

### Epic 5 — When things go wrong

**S-5.1 — Errors speak human.**
As Oscar, when Scout cannot run, Crew says "Profile *scout* is not logged in
to its provider — run `<command>`" or "Profile *scout* was deleted outside
Crew" — not a generic offline badge.

- AC1: The three distinguishable failure classes — binary missing, profile
  missing, provider unauthenticated — each render a distinct, actionable
  message.

**S-5.2 — Clean offboarding.**
As Oscar, deleting agent Builder asks: "Keep or delete profile *builder*
(memory + skills)?" Keep enables re-hiring later; delete leaves no orphan.

- AC1: Agent deletion offers an explicit keep/delete choice for the bound
  profile; "delete" removes the profile store; "keep" leaves it intact and
  re-attachable.
- AC2: Profile deletion is never silent and never triggered by anything
  other than this explicit choice.

### One month later (definition of success)

Oscar opens Crew each morning. Scout and Builder work the board's `Working`
column. In that month he changed Scout's model once (one command, no
restart), taught Builder three skills from his terminal (Crew inherited them
automatically), and hired a third agent in two minutes. He never opened a
config file, never read a crash log, and never wondered which model an agent
was running — the answer always lives in exactly one place.

---

## 4. Current state — verified evidence

Facts verified in this checkout and on this machine (2026-08-04). These are
the inputs to the spikes, not substitutes for them.

### Crew/Buzz side

- Hermes is a tier-2 preset: id `hermes`, command `hermes-acp`, docs link —
  `desktop/src-tauri/src/managed_agents/discovery.rs:1594`. The probe is
  PATH-only. **On this machine `hermes` is installed but `hermes-acp` is
  not, so the preset reports Hermes as not installed while it is.**
- Tier-1 entries live in `KNOWN_ACP_RUNTIMES`
  (`discovery.rs:71`) with ~25 capability fields: install commands, auth
  probes (`auth_probe_args`), readiness requirements
  (`required_normalized_fields`), `default_env`, `skill_dir`,
  `model_env_var`/`provider_env_var`/`provider_locked`.
- **Precedent for "runtime owns the model":** the Claude tier-1 entry sets
  `model_env_var: None`, `provider_env_var: None`, `provider_locked: true`
  (`discovery.rs:124-126`). A Hermes tier-1 entry can copy this shape.
- Default agent args are hardcoded per known command
  (`default_agent_args`, `discovery.rs:462`); unknown commands get none —
  a bare `hermes` command today would spawn without the `acp` subcommand.
- Model injection path: effective config resolves
  record → persona → global default
  (`managed_agents/global_config/mod.rs:42-45`), then spawn sets
  `BUZZ_ACP_MODEL` (`managed_agents/runtime.rs:766`). The harness stores it
  as `desired_model` and re-applies it after **every** `session/new` via
  `session/set_model` or `set_config_option`
  (`crates/buzz-acp/src/pool.rs:179`, `pool.rs:1036`). Failures are
  non-fatal (agent falls back to its own default model).
- `BUZZ_ACP_MODEL` is deliberately **not** a reserved env key
  (`managed_agents/env_vars/tests.rs:135`) — it can also arrive through
  global/persona/agent env-var maps. Contract S-2.2/AC2 must therefore hold
  across *all* layers, not just the model field.
- `buzz-acp` already special-cases Hermes: spawns matching the normalized
  identities `hermes | hermes-agent | hermes-acp` get
  `HERMES_ACP_SKIP_CONFIGURED_MCP=1` by default (persona `extra_env` can
  override) — `crates/buzz-acp/src/config.rs:714-719`, applied before
  `extra_env` in `crates/buzz-acp/src/acp.rs:493-497`. Upstream accepted
  this for block/buzz#3355, which signals upstream receptiveness to Hermes
  support.
- Custom (tier-3) harness JSON exists as an escape hatch, but the id
  `hermes` is reserved via `BUILTIN_IDS` — a custom file cannot shadow the
  preset (see `AGENTS.md` § BYOH).
- Spawn env also carries `BUZZ_ACP_AGENTS = record.parallelism`
  (`runtime.rs:728`) — a Hermes agent with parallelism N would run N
  `hermes acp` subprocesses **against one profile**.

### Hermes side

- A profile is selected by `-p/--profile` (stripped pre-argparse, valid
  before or after the subcommand — `hermes_cli/main.py:517`), by
  `HERMES_HOME` directly, or by a wrapper alias. Profile names are
  normalized/validated and missing profiles raise
  "Profile 'x' does not exist. Create it with: hermes profile create x"
  (`hermes_cli/profiles.py:2210-2226`).
- The ACP adapter resolves everything through `get_hermes_home()` and loads
  the profile's own `.env` (`acp_adapter/entry.py:102-107`) — profile
  selection fully applies to ACP mode.
- `hermes acp --check` exists but verifies dependencies/imports, not
  provider auth (verified: exits 0 on this machine). There is **no known
  headless auth-status command with documented exit-code semantics** — this
  is a Hermes-side ask.
- Profile lifecycle CLI exists: `hermes profile
  create|delete|list|show|export|import|alias` (verified via `--help`).
  Non-interactive behavior of `create`/`delete` (flags, exit codes,
  prompts) is unverified — spike S0-3.
- This machine: profiles `default`, `builder`, `scout` exist, each with a
  distinct configured model; `hermes` is on PATH at
  `~/.local/bin/hermes`.

---

## 5. Target model

### Ownership map (who owns what, permanently)

| Concern | Owner | Crew's role |
| ------- | ----- | ----------- |
| Model + provider + fallbacks | Hermes profile | Display only (read from ACP session catalog) |
| Memory, skills, plugins | Hermes profile | Display/none |
| Provider credentials | Hermes profile | Readiness probe only |
| Agent identity on relay (keypair) | Crew | Unchanged from today |
| Placement: channels, mentions, threads, worktrees | Crew | Unchanged from today |
| System prompt / persona text, team instructions | Crew | Unchanged (prompt-layering via harness) |
| Turn scheduling, steering, board state | Crew | Unchanged |
| Profile lifecycle (create/delete) | Hermes CLI, **invoked explicitly by Crew UI** | Orchestrates with visible consent |

### The binding

- Exactly one profile per agent record. The binding is stored on the Crew
  side (agent record) as the profile *name*, not an absolute path — names
  survive machine moves better and match `hermes -p` semantics.
- Spawn shape: command `hermes`, args `-p <profile> acp` (exact placement
  validated in spike S0-1). The command basename must remain `hermes` (or
  `hermes-acp`) so the existing `buzz-acp` env-guard keeps matching —
  wrapper aliases like a `scout` binary are forbidden for this reason.

---

## 6. Proposed locked decisions

To be appended to [`DECISIONS.md`](../DECISIONS.md) upon approval (numbers
assigned then). Work under this feature may not reopen them.

- **P-1 — Agent = profile.** Every Crew agent on the Hermes runtime binds
  1:1 to a named Hermes profile. The profile owns model, provider, memory,
  skills, and credentials. Crew never stores a competing copy of any of
  these.
- **P-2 — No model UI for Hermes agents.** Crew renders model/provider as
  read-only, profile-sourced information. The runtime model picker (live
  ACP switch) remains available as a session-scoped escape hatch only; it
  resets on respawn and is never persisted by Crew.
- **P-3 — No model injection.** For runtime=Hermes, the spawn environment
  must not contain `BUZZ_ACP_MODEL` from any layer (field resolution or
  env-var maps). Enforcement is code, not convention.
- **P-4 — Tier-1 promotion happens upstream.** The `KnownAcpRuntime` entry
  for Hermes is contributed to `block/buzz` and received via normal
  upstream sync. Crew carries at most a temporary, additive shim until the
  sync lands; permanent fork drift in `discovery.rs` is not acceptable.
- **P-5 — Spawn identity is preserved.** Hermes agents spawn with command
  basename `hermes` (or `hermes-acp`), never via renamed wrappers, so
  runtime-identity matching (env guards, future tier-1 detection) holds.
- **P-6 — Profile lifecycle is explicit.** Crew may invoke
  `hermes profile create/delete` only as a direct, visible consequence of a
  manager action, with the outcome shown. No silent or implied profile
  mutations. Deletion additionally requires the keep/delete choice
  (S-5.2).
- **P-7 — The manager's default profile is not an agent.** `~/.hermes`
  (profile `default`) is treated as personal; binding it to a Crew agent
  requires explicit confirmation and is discouraged in UI copy.

---

## 7. Scope split

### 7.1 Crew-owned (this fork)

In scope:

- Profile binding field on Hermes agent records; create-flow integration
  (pick existing / create new profile).
- Model/provider field suppression + read-only "provided by profile"
  presentation for runtime=Hermes.
- `BUZZ_ACP_MODEL` suppression for runtime=Hermes (P-3) — mechanism decided
  after spike S0-5 (preferred: additive resolver hook; upstream edit only
  with justification per thin-fork rules).
- Readiness surface: binary present → profile exists → provider
  authenticated, with distinct messages (S-5.1).
- Delete flow with keep/delete profile choice (S-5.2).
- Duplicate-binding guard (S-4.1).
- `docs/crew` documentation of the whole model.

Out of scope (Crew):

- Editing Hermes profile contents (model, skills, memories) from Crew UI.
- Multi-machine profile sync.
- Any change to non-Hermes runtimes.

### 7.2 Upstream `block/buzz` (contribution)

Proposed tier-1 entry (mirrors the Claude shape):

- `id: "hermes"`, `commands: &["hermes-acp", "hermes"]`,
  `default_agent_args` mapping `hermes` → `["acp"]`.
- `model_env_var: None`, `provider_env_var: None`, `provider_locked: true`.
- `mcp_command: Some("buzz-dev-mcp")` — **required, not optional**
  (verification 0006): Hermes' own terminal sandbox strips `BUZZ_*`
  credentials (`_HERMES_PROVIDER_ENV_BLOCKLIST`) and its
  `env_passthrough` config refuses them by design, so the only reply
  path is the harness-provided MCP server, which receives `BUZZ_*` via
  `session/new` `mcpServers[].env`. Same shape as the Codex entry.
- `default_env: &[("HERMES_ACP_SKIP_CONFIGURED_MCP", "1")]` — relocating
  the guard from `buzz-acp` to the declarative runtime entry.
- `auth_probe_args`: blocked on the Hermes-side probe (7.3); the PR ships
  only after a verified command exists (contributor guide requires
  vendor-verified entrypoints).
- Remove `hermes` from `PRESET_HARNESSES`; keep the `BUILTIN_IDS`
  reservation.

Also upstream-worthy, separately: profile-awareness is **not** proposed
upstream (upstream has no profile concept; that is Crew's value-add).

### 7.3 Hermes-owned (asks to Nous Research)

- **Auth probe (concrete, from spike 0010):** `hermes auth status
  <provider> --check` (or equivalent) that exits non-zero when the
  provider is not authenticated, honors `-p`, and needs no TTY. Today
  `auth status` prints `logged in`/`logged out` but always exits 0
  (`hermes_cli/auth_commands.py:509`), which fails Buzz's exit-code-only
  probe contract (`readiness/cli_probe.rs:68-72`). Without it, tier-1
  auth badges cannot be truthful.
- **Credential isolation switch:** a per-profile way to opt out of the
  global-root credential-pool fallback (spike 0010 finding), so a
  public/anyone-facing agent's profile cannot read the manager's pooled
  credentials.
- Documented non-interactive `hermes profile create <name>` /
  `delete <name> -y` behavior — **verified working in spike 0011**; the
  ask reduces to documenting the contract (exit codes, name regex
  `[a-z0-9][a-z0-9_-]{0,63}`, non-TTY `delete` auto-cancel exiting 0) so
  Crew can depend on it across versions.
- (Nice-to-have) A documented statement on multi-process safety of one
  profile's session store (WAL observed in spike 0012); decides whether
  C-11's parallelism cap can ever be lifted by default.

---

## 8. Contracts

Scenario table (each row becomes at least one RED test before its slice is
implemented). "Hermes agent" = agent record with runtime=Hermes bound to
profile P.

| # | Scenario | Initial state | Action/event | Expected result | Forbidden side effect |
| - | -------- | ------------- | ------------ | --------------- | --------------------- |
| C-01 | Catalog truth | Only `hermes` on PATH | Open runtime catalog | Hermes shown available | "Not installed" while binary exists |
| C-02 | Profile-bound spawn | Profile `scout` exists | Dispatch mention to agent | Subprocess runs with profile `scout` home; reply arrives in channel | Any read/write to `~/.hermes` default store |
| C-03 | Missing profile | Profile deleted outside Crew | Readiness / dispatch | Distinct missing-profile state and repair path; no zombie spawn loop | Generic offline badge with no cause |
| C-04 | Profile model/persona editor | Bound Hermes profile exists | Open create/edit | Editable write-through model/provider and populated `SOUL.md` editor; no Crew copy | Read-only "provided by profile" row |
| C-05 | No model injection (fields) | Global default model set to a valid Hermes model id | Spawn Hermes agent | Env has no `BUZZ_ACP_MODEL`; session model = profile's model | Silent model switch to global default |
| C-06 | No model injection (env maps) | `BUZZ_ACP_MODEL` set in global/persona/agent env vars | Spawn Hermes agent | Value stripped/ignored for Hermes runtime | Model override applied |
| C-07 | Profile-side model change | Agent idle | Change model through Crew/profile; next mention | Read-back value appears and next fresh session uses it; `!rotate` forces one | Crew restart required |
| C-08 | Memory isolation | Agents A(P1), B(P2) in one community | Teach A a fact; ask B | B cannot recall it | Cross-profile recall |
| C-09 | Skill inheritance | Skill installed into P outside Crew | Dispatch turn | Skill usable in turn | Crew-side re-install needed |
| C-10 | Duplicate binding | Agent A bound to P, running | Create agent B bound to P | Warn or block with explanation | Silent double-bind |
| C-11 | Parallelism guard | Hermes agent, parallelism=4 requested | Save/spawn | Behavior per S0-4 verdict (cap or proven-safe) | Corrupted profile session store |
| C-12 | Unauthenticated profile | P exists, provider logged out | Readiness view / spawn attempt | Honest `auth-unknown`; no false login badge or headless probe | Opaque crash loop or claimed auth success |
| C-13 | Offboarding | Agent bound to P | Delete agent, choose "keep" | Record gone; P intact and re-attachable | Profile deleted |
| C-14 | Offboarding-delete | Agent bound to P | Delete agent, choose "delete" | P removed after explicit confirmation | Deletion without the choice |
| C-15 | Non-Hermes unaffected | Goose/Claude/Codex agents | All above flows | Behavior identical to today | Any regression |
| C-16 | Upstream sync safety | Fork shim present; upstream tier-1 lands | Run sync per runbook | Shim retired; no duplicate catalog entry | Two Hermes entries or id clash |
| C-17 | Env guard preserved | Profile-bound spawn via `hermes -p X acp` | Inspect child env | `HERMES_ACP_SKIP_CONFIGURED_MCP=1` present (persona can override) | Guard lost due to command shape |

---

## 9. Slices and implementation plan

### Slice 0 — Spikes (no production code) — **COMPLETE 2026-08-05**

Records: [`spikes/0009`](../spikes/0009-profile-bound-hermes-acp-spawn.md),
[`0010`](../spikes/0010-hermes-headless-auth-probe.md),
[`0011`](../spikes/0011-headless-hermes-profile-lifecycle.md),
[`0012`](../spikes/0012-one-profile-concurrent-acp.md),
[`0013`](../spikes/0013-buzz-acp-model-leak-suppression.md).

- **S0-1 — Profile-bound spawn end-to-end.** **PASS** (spike 0009).
  `hermes -p <profile> acp` completes initialize → session/new → prompt →
  reply; state isolates to the profile; root store untouched; C-17 env
  guard and arg passthrough verified via existing `buzz-acp` tests. The
  probe spoke ACP directly (no relay); the relay round-trip moves to the
  Slice 1 live probe.
- **S0-2 — Readiness probe semantics.** **FAIL** (spike 0010).
  `hermes auth status` exits 0 in both states; no current command meets
  Buzz's exit-code probe contract. Consequences: upstream entry ships
  `auth_probe_args: None` + `login_hint`; C-12 degrades to
  binary+profile+model-configured checks; concrete Hermes ask filed in
  §7.3. **Bonus finding:** fresh profiles read the manager's pooled
  credentials via a global-root fallback — see the S-4.2 correction
  below.
- **S0-3 — Headless profile lifecycle.** **PASS** (spike 0011).
  Create/delete fully headless; distinct exit codes for invalid name,
  duplicate, missing; name regex `[a-z0-9][a-z0-9_-]{0,63}`; missing
  profile at spawn → exit 1 with actionable message (C-03's error
  class). Caution: `delete` without `-y` auto-cancels with exit **0** on
  a non-TTY — orchestration must always pass `-y` and verify by
  directory absence.
- **S0-4 — One profile, N processes.** **PASS, bounded** (spike 0012).
  Two concurrent ACP processes on one profile: correct isolated replies,
  no lock errors (store is WAL). C-11 stance: default `parallelism = 1`
  for Hermes agents, no hard block, soak test required before any
  N>1 default.
- **S0-5 — Model-leak enforcement point.** **PASS** (spike 0013). Three
  leak paths found (field resolution `runtime.rs:766`; runtime-metadata
  env — inert now, closed permanently by the upstream entry's
  `model_env_var: None`; user env maps written last at
  `runtime.rs:859`). Enforcement: one last-write `env_remove` guard in
  an additive Crew module + 1–2 call-site lines, hashed consistently
  with `spawn_config_hash`.

### Slice 1 — Manual profile-bound agents (smallest working slice) — **VERIFIED 2026-08-05**

- Evidence:
  [`verification/0006`](../verification/0006-hermes-slice1-live-roundtrip.md)
  — live relay round-trip (C-02) and strict C-07 (profile model change
  picked up by a running adapter after `!rotate`, no respawn) both PASS.
- Runbook: [`HERMES.md`](../HERMES.md); decisions locked as D-019;
  per-profile tier-3 harness JSONs registered for `builder` and `scout`.
- **Key operational finding:** `BUZZ_ACP_MCP_COMMAND` (buzz-dev-mcp) is
  mandatory for Hermes agents — Hermes' sandbox strips `BUZZ_*` from its
  own terminal tool, so the MCP server is the only reply path (§7.2
  updated).

### Slice 2 — Crew UI: binding, readiness, profile editing — **SHIPPED in #104, #134, #118**

- Shipped: profile field and create-in-place lifecycle, named readiness and
  repair routing, duplicate-binding guard, `BUZZ_ACP_MODEL` suppression,
  profile model/provider write-through, exact-byte `SOUL.md` editing,
  skippable persona-at-birth, and optional Layer-3 instructions. Non-Hermes
  runtimes retain their existing adapter model controls and no persona editor.
- Remaining: a truthful Hermes headless auth probe (C-12), live session model
  discovery, and the upstream tier-1 sync work described in Slice 3.
- Upstream-file edits (persona editor, create flow, spawn resolver) must
  each carry the "why composition is insufficient / expected diff size"
  justification per the FEATURE template.

### Slice 3 — Upstream tier-1 PR

- Preconditions: S0-2 resolved (probe exists) or probe field deferred;
  Slice 1 evidence linked in the PR as real-world usage.
- Content: PR to `block/buzz` per §7.2 (entry + default args + env
  relocation + preset removal + logo continuity + tests per the BYOH
  contributor guide). After it lands, receive via
  [`UPSTREAM-SYNC.md`](../UPSTREAM-SYNC.md); retire any Slice 2 shims that
  the tier-1 entry obsoletes (C-16).

### Slice 4 — Lifecycle completion

- Preconditions: S0-3 PASS; C-13, C-14 RED first.
- Content: create-profile-in-place during agent creation; delete flow with
  keep/delete choice; orphan detection ("profile missing" class from
  C-03 gains a repair path).

---

## 10. Edge cases

- **Invalid input:** profile names are normalized/validated by Hermes;
  Crew must reject names Hermes would reject *before* spawn (mirror rules
  or call a validation command — S0-3). Absolute-path bindings are
  rejected (names only).
- **Duplicate/replay:** double-binding one profile (C-10); re-creating an
  agent with the name of a deleted one whose profile was kept (must offer
  re-attachment, not fail).
- **Ordering/concurrency:** N>1 parallelism on one profile (C-11/S0-4);
  two agents' turns interleaving on distinct profiles must stay isolated
  (C-08); steering/interrupt mid-turn must not strand profile session
  state.
- **Cancellation/recovery:** harness respawn after crash must reuse the
  same profile cleanly; a profile deleted while its agent is mid-turn
  fails the turn with the C-03 error class, not a hang.
- **Provider/platform:** Windows npm shims (`hermes-acp.cmd`) already
  normalize in `buzz-acp` (`config.rs` tests); Crew's slice is
  macOS-first per current product scope — Windows path shapes are
  documented as untested.
- **Upstream compatibility:** preset id `hermes` is reserved in
  `BUILTIN_IDS`; Crew's tier-3 files must use non-reserved ids
  (`hermes-<profile>`), and C-16 covers the sync that lands upstream
  tier-1. The `skill_dir` tier-1 field assumes a static directory; Hermes
  skills are per-profile, so `skill_dir` stays `None` upstream and skill
  deployment remains Hermes-owned (S-3.3 works through the profile, not
  through Buzz skill deploy).
- **Security:** S-4.2 (public agents / minimal profiles); no
  install-shell-commands in preset/custom definitions (upstream security
  guarantee) — profile provisioning is a Crew UI action calling the local
  CLI with consent (P-6), which must be presented as such and be
  auditable.

---

## 11. Files

### Add

- `docs/crew/features/0001-hermes-first-class-runtime.md` (this file)
- `docs/crew/spikes/00XX-*.md` — five records (S0-1 … S0-5)
- `docs/crew/HERMES.md` — Slice 1 runbook (conventions + manual flow)
- Slice 2+: additive Crew modules for binding/readiness (paths decided in
  the slice plan after S0-5)

### Edit upstream files (this fork)

- None in Slices 0–1.
- Slice 2: candidate edits to persona editor, create flow, and spawn
  resolver — each requires its own justification and diff-size estimate
  before approval (template § Files).

### Upstream repo (not this fork)

- `block/buzz` PR per §7.2.

### Delete

- None. (Slice 3 retires temporary shims via normal sync, not deletion of
  upstream files.)

---

## 12. Verification

- **Focused tests:** per-contract tests C-01…C-17 introduced RED with
  their slice; env-guard and arg-shape tests colocated with existing
  `buzz-acp` spawn tests only if upstream-bound (else Crew-side).
- **Integration:** Slice 1 live-relay probe (mention → Hermes reply) on an
  isolated relay, mirroring the existing Project-contract live test
  pattern; memory-isolation probe (C-08) scripted against two profiles.
- **E2E/live smoke:** Slice 2 Playwright specs for create/edit/readiness
  surfaces (mock bridge), per desktop E2E conventions.
- **Upstream quality gates:** `just ci` for any fork Rust/TS edit;
  upstream PR runs upstream's own gates.

## 13. Rollback

- Slices 0–1: delete docs + tier-3 JSON files; no relay or app-state
  migration exists to undo.
- Slice 2: UI/guard code is additive-first; disabling the Hermes-specific
  branches restores today's behavior. Agent records carrying a profile
  binding degrade to plain custom-command agents (binding field ignored).
- Slice 3: upstream entry arrives via sync; if it must be reverted, the
  preset tier-2 entry shape is restored upstream — Crew is not the owner.
- No step stores authoritative data outside the profile (Hermes-owned) or
  existing Crew records, so rollback never loses relay data.

## 14. Approval

- Slice 0 spikes may start immediately (no production code).
- Slices 1–4 each require manager approval on the slice plan after the
  relevant spikes and RED contracts exist, per D-008.
- The upstream PR (Slice 3) additionally requires manager sign-off on the
  exact proposed entry before submission.

## 15. Unresolved questions

Resolved by Slice 0 (2026-08-05): ~~auth probe existence~~ (spike 0010 —
none today; degraded readiness + Hermes ask), ~~parallelism safety~~
(spike 0012 — bounded PASS; default cap 1), ~~P-3 enforcement point~~
(spike 0013 — last-write guard + upstream `None`s), ~~fresh-profile
contents~~ (spikes 0010/0011 — minimal-but-credential-fallback; explicit
isolation step needed).

Still open:

1. **Spike record vs feature numbering:** spike files are chronological
   (0009–0013) while this doc's slice labels (S0-1…S0-5) are logical —
   keep the cross-reference table in §9 authoritative.
2. Whether the runtime model picker (live ACP switch) should be hidden
   entirely for Hermes agents instead of kept as an escape hatch —
   current stance: keep, session-scoped (P-2); revisit after Slice 1
   usage.
3. Profile name ↔ agent name coupling: enforce `agent slug == profile
   name` for legibility, or allow free binding? Current stance: free
   binding with matching-by-default in the create flow.
4. C-07 strict reading: does a *long-lived* adapter pick up a
   profile-side model change at the next `session/new` without respawn?
   (Spike 0009 verified pickup across process restarts only; buzz-acp
   rotates sessions within one adapter process.) Verify during the
   Slice 1 live run.
5. Slice 4 UX: `--no-skills` (empty profile) vs bundled-skills default
   at Crew-driven profile creation.
