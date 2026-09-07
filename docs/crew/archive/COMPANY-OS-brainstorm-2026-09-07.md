# CompanyOS — direction notes (living brainstorm)

- **Audience:** the founder (Oscar) and any agent brainstorming or planning a
  CompanyOS topic in a later session
- **Status:** Brainstorm record, **not** a plan and **not** accepted decisions.
  Nothing here overrides [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md) or
  [`DECISIONS.md`](DECISIONS.md). When a topic below matures, it becomes a
  spike/feature doc and, if needed, a `D-0xx` entry.
- **Started:** 2026-09-07 (session 1)

## How to use this file

Each topic has: **Direction** (what we currently believe), **Already exists**
(the Buzz/Crew seam it hangs off), **Open questions** (what a dedicated
session should go deep on). Later sessions: pick one topic, go deep, append a
dated "Session N" note under it, and promote conclusions to spikes/decisions.
Do not rewrite earlier sessions' notes; append.

## Founder framing (session 1, verbatim intent)

- Crew today is Slack-style: assign work to agents, wait for replies.
- Dream: a **CompanyOS / superapp** — talk to agents like employees, have
  departments, see what agents are doing, agents work with each other, and
  check / answer email from the same place. No switching between agent apps.
- Personal project: **the founder is the only human** in the company.
- Founder is a developer: sometimes wants to *watch* an agent think and work
  (Codex / Claude Code style), not only read Slack-style replies.
- "Agents work together" means **talking like colleagues**, not two agents
  editing one repo in parallel.

## Facts locked by existing decisions (do not re-litigate here)

| Fact | Source |
| --- | --- |
| Office = product lens; no separate Office entity | FOUNDER-PRODUCT, "Channel vs Office" |
| No org chart product; roles are channel-scoped and must change behavior | D-043, D-044, D-069 |
| Specialists are called by name via mention → ACP wake; no new kind | D-071, D-072, spike 0055 |
| Thread Workbench = session-shaped layout over the same thread (two doors) | D-055 |
| One declared plan snapshot per agent, thread-visible | D-056 |
| Founder is the client; Gate C accept/reject | D-070 |
| Hermes is the default employee; other engines via Buzz contracts | D-025 |
| Every Crew feature attaches to a Buzz seam (event kind, type, command) | AGENTS.md |

---

## T0 — The core reframe: work-centric, not conversation-centric

**Direction.** The "switching between agents" pain is a navigation-model
problem, not a missing feature. Slack-style navigation is *conversation
first* (pick a channel/DM, then read). CompanyOS navigation is *work first*:

| Slack-style (today) | CompanyOS (target) |
| --- | --- |
| Sidebar = channels / DMs | Sidebar = departments + running work + Needs you |
| Founder goes to find an agent | Agents report into one place; founder reviews |
| Agent status scattered per channel | One "who is doing what" view |
| Founder orchestrates | Founder is client / CEO: states intent, answers Needs you, accepts or rejects (D-070) |

The backend does not change. This is a new **Home** and a new sidebar
grouping over existing stores.

**Superapp rule (proposed).** Every "app" inside CompanyOS is:
`(events on the relay) + (an agent tool) + (a lens in desktop)`.
Never a standalone UI module. Adding the N-th integration costs one bridge,
not one product. This is the same rule as AGENTS.md "prefer Nostr events over
new HTTP endpoints", applied to product.

**Already exists.** `features/home`, `features/pulse`, `needsYouStore`,
`activeAgentTurnsStore`, `agentWorkingSignal`, `agentReceiptStore`,
`conversationOutcomeLedger`.

**Open questions.**
- What are the 3–4 blocks on Home? Proposal: *Needs you* / *Running (who,
  where, how long)* / *Done, awaiting Accept* / *Departments*.
- Does Home replace the channel list as the default route, or sit beside it?
- Mobile: same Home, fewer blocks?

---

## T1 — Departments = channels with resident agents

**Direction.** A department is **not** a new object. It is a channel that
has (a) channel roles (39003 / D-043) that change behavior, and (b) one or
more resident Hermes profiles whose persona and memory make them "the
Marketing person". Creating a department = applying a **channel template**:
create channel + assign roles + spawn/attach profiles with a persona pack.

This is compatible with D-069 because a department changes *who may do what
in this room*; it is not a decorative tree.

Because the founder is the only human, a department's membership is
`founder + N agents`. No human membership UX is needed day one.

**Already exists.** `features/channel-templates`, `crates/buzz-persona`
(persona packs), Hermes profiles (feature 0001), D-072 (CoS as channel
intake; specialists called by name).

**Open questions.**
- Department template shape: name, roles, default profiles, default
  persona, welcome kickoff message?
- One CoS per department, or one company-wide CoS who routes to departments?
- Cross-department work: does the CoS mention an agent in another channel,
  or open a thread in the other department's channel? (Mention wake is
  channel-scoped; see spike 0055 fail-closed rules.)
- How does the sidebar render a department differently from a plain
  channel (facepile of residents + live status)?

---

## T2 — Watching agents work: Office ↔ Focus (the founder's top priority)

**Direction.** Two grains of the *same thread*, one keystroke apart:

- **Office grain** (Slack-style): kickoff, receipts, evidence, questions,
  results. Calm. What a manager reads.
- **Focus grain** (Codex / Claude Code style): full-screen session on one
  thread — streaming assistant text, thinking, tool calls with inputs and
  outputs, declared plan (D-056), terminal / browser / simulator panes,
  Stop / Steer, target chip. What a developer watches.

D-055 already establishes this ("one thread, two doors") and the
`features/workbench` code exists. The CompanyOS work is to make Focus grain
**as good as a native agent CLI**, and to make the door frictionless:

1. From any thread row (Home, channel, department) → one action → Focus.
2. Focus shows *everything the ACP session emits*, not a filtered subset:
   thinking blocks, tool call cards (collapsed by default, expandable),
   file diffs, shell output, plan updates, sleep/wake.
3. Tool pane (browser / sim / terminal) docks beside the transcript in
   Focus, so "see what it is doing" includes what it is driving.
4. Escape → back to Office grain at the same thread; unread and scroll
   position preserved.
5. Multi-agent thread in Focus: transcript interleaves agents with clear
   ownership; target chip picks who a message is aimed at (D-055 item 3).

**Already exists.** `features/workbench` (rail, transcript rows incl.
`observer` items from kind 24200 frames, `user-input`, `sleep-wake`,
`catch-up`), `features/tool-pane`, `features/terminal`, D-056 plan
snapshots, `activeAgentTurns*`, ACP observer frames (ephemeral, owner-only).

**Gap to verify (needs a session with the running app).**
- Do observer frames carry thinking / reasoning text today, or only tool
  activity? If only tools, decide whether thinking is streamed (ephemeral)
  and under which existing frame type.
- Is the Focus toggle discoverable from every thread entry point, or only
  via `ThreadWorkbenchEntryButton`?
- Does Focus keep working when the founder navigates away and back
  (session liveness, catch-up row)?
- Keyboard: one chord to toggle; Stop / Steer without mouse (AGENTS.md
  review rule 8).

**Open questions.**
- Should Focus be a *route* (deep-linkable, mobile-shareable) or a *mode*
  of the current thread view? D-055 says both doors select the same
  destination; confirm the URL shape.
- Multiple Focus panes side by side (two agents at once)? Probably later.
- Retention: observer frames are ephemeral. If the founder opens Focus
  late, what is reconstructable? (`workbenchCatchUp`, receipts, plan
  snapshot.) Decide whether a bounded local archive is worth it
  (`features/local-archive` exists for something similar).

### Session 1 code audit (2026-09-07) — what exists vs. what is missing

Founder answer: Focus must show **both** thinking text and tool activity —
a real chat UI with the agent.

**Exists — data layer (upstream Buzz, Rust; do not touch).**
- `crates/buzz-acp` forwards every ACP `session/update` into observer
  frames (kind 24200, NIP-44, owner-only): `agent_message_chunk`,
  `agent_thought_chunk` (thinking — confirmed present, see
  `observer_chunk_key_and_text` in `lib.rs`), `tool_call` /
  `tool_call_update` (args, status, result), `plan`, permission,
  lifecycle. Chunks are coalesced before publish.
- Frames are ephemeral on the relay, but the desktop `local-archive`
  feature persists them in SQLite (default-on; caps in
  `observerRelayRetention.ts`: 3000 events × 12 channels, +5 min after
  idle). Late-open reconstruction is therefore mostly solved.

**Exists — render layer (upstream Buzz, React).**
- `TranscriptItem` union in `agents/ui/agentSessionTypes.ts` already has
  `message` / `thought` / `plan` (with todos) / `tool` (args, result,
  status, isError) / `lifecycle` (status, permission, error) /
  `metadata` (raw prompt rail).
- Presenters exist per class: `ThoughtActivity`, `ToolActivity`,
  `PlanActivity`, `LifecycleActivity`, `RawRailActivity`; tool details
  include `ShellCommandBlock`, `ViewImageToolPreview`, `TodoToolSummary`.
- `AgentSessionThreadPanel` (upstream) renders this transcript for one
  agent in the channel's right panel, with Stop and Escape.

**Exists — Focus grain (Crew, D-055).**
- Route `/workbench/$channelId/$threadRootId`; rail by thread / by agent;
  transcript interleaves message + observer + user-input + sleep/wake +
  catch-up; composer has Stop / Steer / target chip; Office ↔ Workbench
  toggle in header (`?office=1`).
- Separately, upstream has a thread **focus drawer**
  (`threadViewModePreference`: `focus | split`, messages only) and the
  Tool Pane `PR · Browser · Sim` docks in that drawer (D-057).

**Missing / broken — the actual work.**
1. **No door.** `ThreadWorkbenchEntryButton` is defined but imported
   nowhere; `goWorkbench` is only called from inside Workbench; no sidebar
   nav, no shortcut. `/workbench` is reachable only by typing the URL.
   This alone explains why Crew still feels Slack-only.
2. **Three overlapping "focus" surfaces, no reconciliation:** upstream
   focus drawer (messages), upstream `AgentSessionThreadPanel` (one
   agent's transcript), Crew Workbench (both interleaved). D-055 names
   Workbench as the second door but does not say where the upstream
   drawer sits relative to it.
3. **Tool Pane not docked in Workbench.** `ChannelToolPane` mounts only
   in `ChannelPane` and `ThreadFocusForgeSplit`. In Workbench the founder
   cannot see the browser / sim / terminal the agent is driving.
4. **Streaming feel unverified.** Whether thought text renders live while
   chunks arrive, whether tool cards collapse by default and expand on
   click, whether the transcript follows the tail during a turn — needs
   the running app.
5. **Keyboard.** No chord for Office ↔ Focus; Stop / Steer are
   button-only (AGENTS.md review rule 8).
6. **Multi-agent ownership.** Observer rows carry `agentPubkey`; visual
   ownership when two agents interleave is unconfirmed.

**Proposed direction (not yet a decision).**
- Workbench is *the* Focus grain. The upstream focus drawer is a
  "wide thread reading" mode, not a competitor; write this down as a
  D-055 follow-up.
- Three doors: thread header button, Home / Needs-you row click, and one
  global chord toggling Office ↔ Focus on the current thread. Esc returns
  to Office and preserves position.
- Dock Tool Pane inside Workbench by reusing `ChannelToolPane` (existing
  seam; matches D-055 item 5 component-reuse contract).
- Display defaults: thought + message always visible; tool card = one
  collapsed line, expand on click; plan pinned at top (D-056).
- First slice must stay tiny: **wire the doors + run the app and audit
  items 4 and 6**. Do not restyle the transcript before seeing it live.

### T2.1 — Latency: ACP done → UI shows done (session 1)

Founder observation: ACP finishes but the app reports "done" 5–10 s later.
Performance is a priority.

**Path (from code):**

```
ACP stopReason
 → [1] TurnCompletionGuard::drop emits observer "turn_completed"   (pool.rs)   ~0 ms
 → [2] ObserverPublishQueue global pacer: 1 frame / 1 s, FIFO,
       one channel per tick, ~64 KB per frame                    (lib.rs OBSERVER_PUBLISH_TICK)
 → [3] NIP-44 encrypt + publish kind 24200                        ~5–50 ms local
 → [4] relay pub/sub → Tauri native client → JS                   ~5–20 ms
 → [5] observerRelayStore → activeAgentTurnsStore "turn_completed"
       → endTurn(); coalescedNotify = microtask + 1 rAF          ~16 ms
 → [6] UI badge off
```

The agent's reply message (kind 40001 via `buzz messages send`) takes a
separate path with no pacer, so the reply often appears *before* the
"done" state.

**Root cause = stage [2].** `turn_completed` is queued behind every
pending thought/tool chunk. Delay ≈ (backlog bytes / 64 KB) × active
channels × 1 s. Three busy agents in three channels → each channel gets a
slot every ~3 s, and the terminal event waits for its channel's backlog.
The pacer is designed for relay quota (120 msg/min/agent shared with chat),
not for latency; the code comment says 1 update/s is "smooth enough".

Stage [5] is not a problem (Crew #286/#287 already split liveness
listeners; rAF coalescing is one frame). Desktop fallbacks
(`LIVENESS_INTERVAL_MS` 10 s, `FRAME_GAP_PAUSE_MS` 20 s, `PRUNE_INTERVAL_MS`
5 s) only matter when `turn_completed` is *lost*; a 20 s+ stall means a
dropped frame, not the pacer.

**Options (cheap → expensive):**

| | Change | Where | Effect |
| --- | --- | --- | --- |
| A | Priority lane: lifecycle events (`turn_started/completed/error`, permission, user-input) jump the queue and go out on the next tick | `buzz-acp/src/lib.rs` `ObserverPublishQueue` (upstream file) | done ≤ 1 s regardless of backlog; tiny bytes, no quota impact |
| B | Adaptive tick (250 ms when lifecycle pending or backlog high, 1 s otherwise) with a token bucket instead of a fixed tick | same file | smoother thought streaming; keeps 120/min ceiling |
| C | Pack multiple channels into one frame | same file | N busy agents stop sharing one slot |
| D | Local IPC path for observer frames (harness and desktop are on the same machine); relay stays the fallback for remote/mobile | new seam; needs a decision (D-003/D-010 exception for ephemeral telemetry) | ms latency, no quota; enables true typing-speed streaming |
| E | Desktop treats the durable receipt (46043) as an additional "done" signal; first of receipt / `turn_completed` wins | `desktop/src/features/agents` (Crew-owned) | badge off when the reply lands, no Rust change |

**Session 1 lean:** A + E first (small, upstream-proposable, no
architecture change), measure, then B if thought streaming still feels
slow. D is the real answer for a Claude-Code-grade live view and belongs
in the T2 deep-dive as an explicit open question.

**Founder decision (2026-09-07):** editing the upstream-owned pacer in
`crates/buzz-acp/src/lib.rs` is **accepted**. Rationale: latency of the
live view is core CompanyOS value; the change is small, self-contained
in `ObserverPublishQueue`, and upstream-proposable. Constraints when it is
implemented: keep the 120/min quota ceiling; keep the byte budget and
drop accounting; add a falsifiable test that a `turn_completed` enqueued
behind a chunk backlog ships on the next tick (AGENTS.md review rule 3);
record the fork delta in UPSTREAM-SYNC so each sync re-verifies it.
Docs only for now — no code until the T2 deep-dive session picks it up.

**To verify before implementing:** when the receipt is published relative
to `stopReason` (E depends on it); actual queue depth under 2–3 concurrent
agents (instrument with `acp::observer` tracing before changing the pacer).

---

## T3 — Agents as colleagues (talking to each other)

**Direction.** Agents already reach each other **through the relay**, not
through MCP: an agent posts a room-visible message with a `p` mention, the
target wakes on the existing ACP path (D-071). CompanyOS keeps that wire
and adds the *social behaviors* of colleagues, mostly through prompts and
thread presentation — not new protocol:

1. **Ask-back.** An agent may ask *another agent* a question in-thread and
   wait for the mention callback (already in `base_prompt.md`: "callback
   mentions"). Today user-input elicitation (46040–42) is agent → human
   only; do **not** extend it to agent → agent unless a session proves the
   mention loop is insufficient.
2. **Visible handoff.** When A calls `@B` for work, the thread renders a
   small card "A → B: what / why / expected back". Candidate seam: a tag on
   the existing message rather than a kind. Must never look like the work
   is done (FOUNDER-PRODUCT: mis-assignment must never look like success).
3. **Disagreement and refusal.** Wrong-role work is refused or escalated in
   the thread (D-043/044 soft enforcement). Colleagues push back; they do
   not silently comply.
4. **Standup / digest.** A department-level "what happened since you last
   looked" is a *lens* over receipts + outcomes, or a scheduled workflow
   (30620) that posts a summary. Prefer lens first.
5. **No bare acknowledgements** (already a hard rule in `base_prompt.md`).
   Colleague-like ≠ chatty.

**Already exists.** Mention → wake, `buzz agents call`, callback-mention
rule, receipts (46043), user-input (46040–42), workflows, CoS intake (D-072).

**Open questions.**
- Does "colleague" need agent → agent *private* talk (DM between agents),
  or must everything be room-visible? Founder-visible-by-default fits the
  "shared thread log" success check; propose **room-visible only**.
- Loop safety: two agents mentioning each other back and forth. What
  bounds exist today (turn budgets, mention gating)? Needs a session.
- Should the handoff card be a message tag, a receipt variant, or pure
  rendering of an existing `p`-mention + content pattern?
- Which behaviors are Hermes-profile prompts vs. shared `base_prompt.md`?

---

## T4 — Email (and other outside-world bridges)

**Direction, session 1.** Start with **Gmail via MCP as a tool of one
Hermes profile** (an "EA" / assistant profile). Reading, triage, and
drafting happen in a department channel (e.g. `#inbox`). **Sending is
always founder-approved**: the agent posts a draft in-thread and asks via
user-input (46040); only after the founder answers does the tool send.

Later, if the founder wants mail *visible to every agent and on mobile*,
move to a **bridge**: inbound Gmail push → events in `#inbox` (one thread
per conversation); outbound = approved draft → bridge send. Same approval
rule. This is the superapp rule applied: event + tool + lens.

Not chosen: a native email client inside Tauri (heavy, anti thin-fork).

**Already exists.** MCP tools in `buzz-dev-mcp` / Hermes tool config,
user-input elicitation, owner-reviewed drafts (`buzz-cli` drafts), workflow
webhooks `/hooks/{id}` (usable for inbound push later).

**Open questions.**
- Which Gmail MCP server (official vs community), credential storage per
  Hermes profile (feature 0001 says credentials are per profile).
- Approval UX: is the existing user-input card enough, or does an outbound
  email need its own "preview + Send" card?
- What does the same pattern look like for Calendar, GitHub notifications,
  Stripe? Probably one doc per bridge later; first prove the email loop.

---

## T5 — Company memory and identity (lightly touched)

**Direction.** Company knowledge lives in the Wiki (30023 company wiki,
30623 repo wiki) and per-profile Hermes memory / engrams (30174). A
"company handbook" the founder edits + agents read is a wiki page, not a
new store. Company identity (name, tone, values) is a wiki page injected
into persona prompts.

**Open questions.** Who owns updates to shared memory (CoS?), and how do
departments avoid contradicting each other.

---

## Suggested order for deep-dive sessions

1. **T2 Focus grain** — highest founder value, mostly desktop, verifiable
   with the running app. Start by auditing what Workbench already shows.
2. **T0 Home** — cheap, pure lens, immediately changes the daily feel.
3. **T1 Departments** — template + persona; unlocks T3 in practice.
4. **T3 Colleague behaviors** — prompts + one thread card.
5. **T4 Email via MCP** — proves the bridge pattern end to end.

## Session log

- **2026-09-07 — Session 1.** Founder + agent brainstorm. Wrote T0–T5.
  Key findings: D-055 already covers the two-grain idea the founder asked
  for; agent-to-agent talk already runs over relay mentions, not MCP;
  departments can be channel templates without violating D-069.
  Deep-dived T2: code audit shows the Focus grain exists but has no door;
  T2.1 traced the 5–10 s "done" delay to the 1 s observer pacer. Founder
  accepted editing the upstream pacer file (docs only, no code yet).
