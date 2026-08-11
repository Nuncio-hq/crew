# Agent Roles, Channel Routing Presets, Role-Scoped Capability — Plan

> Issue: [Nuncio-hq/crew#116](https://github.com/Nuncio-hq/crew/issues/116)
> Workflow: `docs/crew/DEVELOPMENT-WORKFLOW.md` — Spike → RED tests → approved plan → implementation.
> This plan sequences the slices and defines the spikes. Implementation tasks inside
> each slice are written to plan-detail level for Slice 0–1 and to scope level for
> Slice 2–3 (they get their own detailed task lists after the spikes report PASS,
> per workflow: "if the spike fails, do not implement around the failure").

**Goal:** Mis-assigned work must never look like success. Owner-assigned roles per
agent, channel routing presets that point at roles, and role-scoped capability at
the harness boundary — all on existing Buzz contracts (D-025).

> **Revision 2 (2026-08-11) — channel-scoped roles.** Issue #116's founder design
> review ("Founder design review — channel-scoped roles") supersedes the issue body
> and revision 1 of this plan: a role is not a global agent attribute but an
> owner-signed **(agent, channel) assignment**. Slice 1 shipped revision 1's global
> model on `feat/issue-116-agent-roles` (PR #120); the rework is part of this round
> and is described under Slice 1R. Everything below Slice 0 is revision 2; the
> Slice 0 spike record is kept verbatim as history.

**Architecture (revision 2):** A role is an owner-signed binding of
`(agent, channel) → role label + role definition`, carried by the **channel
canvas** (`KIND_CANVAS` = 40100, `h`-tag scoped, already founder-editable and
already injected once per channel session). Role labels are free-form founder
text; meaning lives in the definition (allowed / not-allowed / redirect) that
travels with the assignment, never in a taxonomy const and never derived from the
name. The same canvas block carries the channel's routing preset
(`work type → required role`). Authority is the canvas event author: a crew block
takes effect only when the canvas event is signed by the agent's owner (founder)
pubkey — everything else is ignored. Channels with no crew block behave exactly as
today. The `10100` `crew-role` projection is demoted to optional CV metadata, not
an enforcement source. Capability is Crew-owned at the spawn boundary, keyed by
role, uniform across engines: the Hermes profile keeps
memory/skills/credentials/model (D-024); it is NOT the capability boundary.

**Tech stack:** `buzz-acp` (context injection, spawn env), `buzz-core` kinds
`30179` / `10100`, desktop managed-agent record + spawn (Phase 02A pattern),
channel canvas, `buzz-dev-mcp` (grant/deny only in this plan).

**Founder decisions:** revision 1's three decisions are already recorded by
PR #120 as **D-028** (roles are owner/founder-signed only — "email promote"),
**D-029** (capability is Crew-owned at the harness/spawn boundary keyed by role,
uniform across ACP engines; D-024 Hermes profile ownership unchanged and
explicitly not the capability boundary) and **D-030** (presets reference roles,
never hard-coded agent names). Revision 2 needs two more, allocated by the
orchestrator: **D-043** (a role is a channel-scoped owner-signed assignment
carried by the channel canvas; supersedes D-028's storage clause, keeps its
authority clause) and **D-044** (capability granularity — decided by spike 0018
evidence, see Slice 3).

---

## Slice 0 — Spikes (throwaway, evidence-first)

Each spike: one decision-changing question, smallest real environment, defined
PASS/FAIL/INCONCLUSIVE up front, record under `docs/crew/spikes/`.

### Spike A — Role record shape and projection

- **Question:** Can the owner-signed managed-agent record (kind `30179`,
  `crates/buzz-core/src/private_managed_agent.rs`) carry a role field whose value
  is projected to a public tag on the agent's `KIND_AGENT_PROFILE` (`10100`)
  without breaking existing consumers, and does the tag survive relay round-trip
  and stay ignorable by non-Crew clients?
- **Method:** Local relay; publish extended `30179` (owner key) + `10100` with
  `["crew-role", "code"]`; cold-read both; open the community in stock Buzz
  desktop to confirm nothing breaks.
- **PASS:** both records round-trip, role readable from `10100`, stock UI
  unaffected. **FAIL:** any consumer rejects/strips the extension.

### Spike B — Role section in per-turn prompt changes behavior

- **Question:** Does an injected role section (allowed / not-allowed /
  refuse-and-redirect + mandatory explicit role-check declaration in the first
  reply) make an off-role mention produce a refusal-with-redirect in the thread
  instead of silent execution?
- **Method:** Engine matrix, not Hermes-only — prompt adherence is a property of
  the model behind the engine, and D-025 requires the generic contract to work:
  two live Hermes profiles (e.g. `code`, `content`) **plus at least one
  non-Hermes ACP engine** (Claude Code or Codex, both previously probed per
  `STATE.md`) on a real relay (reuse verification 0006 setup,
  `BUZZ_ACP_MCP_COMMAND=buzz-dev-mcp` set). Script ~10 mentions per agent:
  on-role, off-role, boundary cases (typo-fix in README, copy change in a code
  dialog). Count accept/refuse/declare outcomes per engine.
- **PASS:** 0 silent off-role executions of repo-mutating work **on every engine
  tested**; refusals name the correct role/agent; declarations appear in-thread.
  **INCONCLUSIVE:** refusals happen but declarations are unreliable → strengthen
  prompt, rerun. A per-engine PASS/FAIL split is itself decision-changing
  evidence (it scopes which engines can hold which roles day one).

### Spike C — Capability grant/deny at spawn, per agent

- **Question:** Can the desktop spawn path grant `buzz-dev-mcp` to one managed
  agent and withhold it from another (per-agent `BUZZ_ACP_MCP_COMMAND`), with the
  denied agent's session having no shell/file tools at all, using the Phase 02A
  per-spawn env mechanism? **And — the generic-contract half:** for a denied
  agent running an engine with NATIVE file/shell tools (Claude Code, Codex),
  is the native write path also blocked, and by what (engine sandbox config vs
  nothing)? `STATE.md` records Codex's native workspace-write being blocked in
  an earlier probe — verify this is configuration we control, not luck.
- **Method:** Two managed agents, one env-granted, one not; ask both to write a
  file in the Project workspace; inspect tool availability and the thread reply.
  Repeat the denied case on at least one native-tool engine and record which
  engine-side permission setting (Claude Code permission mode / Codex sandbox
  policy) governs the outcome.
- **PASS:** denied agent has no dev tools and says so honestly; granted agent
  succeeds; for native-tool engines, either the native path is deniable via
  engine config the spawn can set, or the limitation is documented as evidence.
  **FAIL:** env cannot be withheld per agent, or denial breaks the turn loop.

**Gate:** all three spikes recorded PASS before any Slice 1 implementation.

### Spike D — Spawn granularity (revision 2, blocks Slice 3's shape)

- **Question:** 0017 proved per-spawn grant/deny of `BUZZ_ACP_MCP_COMMAND` but not
  what a spawn corresponds to. Can capability differ **per channel-session** of one
  agent, or only per agent process?
- **PASS:** Slice 3's floor is channel-scoped — the assignment in a channel decides
  that channel session's dev-mcp grant and engine permission flag.
  **FAIL:** the floor is per agent process (union of that agent's assignments),
  documented honestly; per-channel discipline stays prompt-level.
- Recorded as `docs/crew/spikes/0018-spawn-granularity-per-channel-session.md`.

---

## Slice 1 — Role per agent (foundation)

User flow: founder assigns a role in the managed-agent edit UI (or via a
founder-authored message flow later); the assignment publishes the owner-signed
record + public projection and posts an announcement message in the agent's home
channel; from the next turn every session of that agent receives the role
section; off-role mentions get an in-thread refusal naming the right role.

Durable output: role visible on the agent profile (UI chip + `10100` tag),
announcement in the room, role-check declarations in threads.

RED contracts first (per workflow), then implementation:

- **Contracts (desktop, additive test files):** role field parse/serialize on the
  managed-agent record; projection builder emits `crew-role` tag; non-founder
  role event is ignored (authority check by pubkey); missing role ⇒ no role
  section injected (current behavior unchanged); role removal clears projection.
- **Contracts (`buzz-acp`):** prompt composer includes role section iff a
  verified owner-signed role exists; section content matches the record; role
  changes take effect on next fresh session (same semantics as model rotation,
  cf. `!rotate` in `docs/crew/HERMES.md`).
- **Implementation order:** record field → projection publish → authority
  check → prompt injection → UI chip + edit control → announcement message →
  display-name convention documented in `docs/crew/HERMES.md`.
- **Taxonomy day one:** small and founder-editable — start `code`, `content`,
  `research`, `ops`; stored as free string, validated list lives in one place.

Definition of done: Spike B scenario rerun on the shipped path with the same
PASS criteria; docs (`HERMES.md`, `STATE.md`, `DECISIONS.md`) updated; `just ci`
green; NuncioCrew Gate on the PR.

Non-goals: hard harness blocking, presets, capability changes.

## Slice 2 — Channel routing preset (derived layer)

Scope (detailed tasks after Slice 1 lands): per-channel `work type → required
role` table; founder-signed only; stored/read via channel canvas + injected
through the existing Project-channel context path; resolution role → current
holder happens at read time in the injected context; agents are instructed to
consult the table before delegating and to route mentions accordingly; every
preset change is itself a message in the room.

RED contracts: non-founder preset edits are ignored; preset referencing a role
with no holder degrades to "ask the founder" (never silent misroute); channels
without presets behave exactly as today.

Non-goals: auto-routing unmentioned messages; preset UI beyond canvas day one.

## Slice 3 — Role-scoped capability (hard floor, honestly scoped per engine)

Scope (detailed tasks after Spike C): map role → dev-mcp grant in the spawn
path (deny = no `BUZZ_ACP_MCP_COMMAND` for that agent); role section text tells
denied agents they genuinely lack repo/shell tools (honest, not theatrical);
contracts assert the env is absent/present per role and that a role change flips
the grant on next spawn.

**Engine honesty rule (D-025 rule 4 applied to capability), updated per Spike C
(0017) evidence:** dev-mcp deny alone is NOT a filesystem floor for ANY engine —
Hermes itself retains native terminal/write_file when MCP is withheld. The real
floor is the **combination proven spawn-settable in 0017**: deny dev-mcp (removes
Buzz-credentialed reply/write path) **plus** the engine-side permission flag at
spawn (Codex `-s read-only`, Claude `--permission-mode plan`; Hermes profile
tool config for the Hermes case). Slice 3 implements role → {mcp grant, engine
permission flag} as one spawn-time capability decision per engine. UI and docs
state the per-engine mechanism honestly — never present capability denial as a
single uniform switch.

Deferred to its own decision + possible upstream tier-1 PR: tool allowlist flag
in `buzz-dev-mcp` (static `tool_router` today, `crates/buzz-dev-mcp/src/lib.rs`)
for partial grants (e.g. read-only research role). Path containment explicitly
out of scope (`crates/buzz-dev-mcp/src/paths.rs` posture unchanged).

## Measurement (runs alongside all slices)

Weekly review over thread logs: count on-role accepts, correct refusals, false
refusals, and any silent off-role execution (must stay 0 for repo-mutating work).
Escalation trigger, decided now: >1 silent off-role repo-mutating execution per
week after Slice 1 ⇒ schedule hard harness-side turn blocking as its own slice.

## Risks / open questions

- LLM adherence to refusal prompts varies by model — Spike B measures the
  starting point; the eval set becomes a regression harness on model changes.
- Role storage detail (extend `30179` content vs sibling owner-signed event) is
  decided by Spike A evidence, not by this plan.
- Boundary-case taxonomy (typo in README vs code change) will need few-shot
  tuning from real transcripts; plan budget for one prompt-iteration pass.
- Thin-fork: all Slice 1–3 changes are additive Crew files or Crew-owned desktop
  code; the only upstream-file risk is the deferred dev-mcp allowlist flag.
