# Crew — product

Crew (product name **NuncioCrew**) is a company on the founder's machine.
People and agents share Buzz rooms; Hermes is the default employee; other
ACP engines plug in through existing Buzz contracts; the founder is the only
real decision-maker. This file is the product north star. Decisions that
change it are recorded in [`DECISIONS.md`](DECISIONS.md); this file is
rewritten in place when direction changes, not appended to.

Read this before planning any product UX, agent runtime, mobile, or
"company" feature. If a plan conflicts with this file, stop and ask the
founder.

## The problem

Agent tooling today is one-agent-at-a-time: open Claude Code, open Codex,
open another terminal, switch. Work, state, and history live in each tool's
private transcript. Nothing sees the whole picture, and the human is the
only integration point.

The founder wants the opposite: **one place** where agents are employees,
work is visible, agents talk to each other, and outside-world tasks (email,
calendar, code review) arrive in the same rooms. Not a Slack clone and not
an IDE — a company.

## What "company" means here

The founder has not worked as a company manager. Do not assume MBA process
or org politics. "Company" means only four practical things:

| Everyday need | In Crew / Buzz |
| --- | --- |
| A place to talk and leave a record | Channel + thread (Nostr events on the relay) |
| Workers who can do tasks | Agents (default: Hermes profiles) |
| Give the right work to the right worker | Mentions + channel roles — never silent mis-assignment |
| Know when work needs a human | Need you / user-input + clear reports in the thread |

The founder states intent, answers Need you, and accepts or rejects
outcomes (Gate C, D-070). Agents investigate, draft, code, report, and hand
off — like colleagues who must explain themselves simply.

Vocabulary to use with the founder: room/channel, thread, employee/agent,
assign (= @mention), Need you, report, desk (desktop), phone (mobile).
Avoid consultant vocabulary and process diagrams the founder did not ask for.

## Platform (locked)

| Choice | Detail |
| --- | --- |
| Keep Buzz backend | Relay, Nostr identity, channels, ACP harness, event log |
| One self-hosted relay | The founder's dev-server over Tailscale is the company's relay; no hosted community |
| Thin fork | Prefer additive Crew files; keep syncing upstream ([`FORK.md`](FORK.md)) |
| Build on top | Every Crew feature attaches to an existing Buzz kind, type, command, or extension point |
| Until | A deliberate platform change is recorded as a new decision |

Rules for implementers (D-025):

1. Optimize product paths for Hermes (hire, spawn, docs, defaults, UX).
2. Use existing Buzz/ACP/Nostr contracts first. Do not invent a parallel
   Hermes-only protocol for room membership, assignment, or results.
3. Extend contracts only when generic Buzz is truly insufficient, and record
   why in `DECISIONS.md`. Prefer extensions other ACP engines can ignore.
4. Never pretend non-Hermes engines have Hermes profile memory.
5. When upstream ships a model for something Crew hand-rolled, Crew moves
   onto it.

## Direction: CompanyOS

Crew today is Slack-style. The target is work-centric, not
conversation-centric:

| Today | Target |
| --- | --- |
| Sidebar = channels / DMs | Sidebar = departments + running work + Needs you |
| Founder goes to find an agent | Agents report into one place; founder reviews |
| Agent status scattered per channel | One "who is doing what" view |
| Founder orchestrates | Founder is client / CEO: intent, Need you, Accept / Reject |

The backend does not change for this. It is a new Home and sidebar over
existing stores.

**Superapp rule.** Every "app" inside Crew is
`(events on the relay) + (an agent tool) + (a lens in desktop)` — never a
standalone UI module. The N-th integration costs one bridge, not one
product.

### Departments

A department is a channel with channel roles that change behavior
(D-043/D-044) and resident Hermes profiles whose persona and memory make
them "the Marketing person". Creating one applies a channel template:
channel + roles + profiles. No org chart product (D-069). The founder is
the only human, so membership is founder + N agents.

### Watching agents work: Office and Focus

Two grains of the same thread, one keystroke apart (D-055):

- **Office** — Slack-style: kickoff, receipts, evidence, questions,
  results. What a manager reads.
- **Focus** — Codex / Claude Code style: full-screen session on one
  thread. Streaming assistant text, thinking, tool calls with inputs and
  outputs, declared plan (D-056), terminal / browser / simulator pane,
  Stop / Steer, target chip. What a developer watches.

Focus must show everything the ACP session emits — thinking and tool
activity both — and must feel as live as a native agent CLI. The data
already streams (observer frames, kind 24200) and the Workbench route
exists; what is missing is a door into it, the Tool Pane docked beside it,
and latency (see D-075 item 6). Tracked in
[#344](https://github.com/Nuncio-hq/crew/issues/344).

### Agents as colleagues

Agents reach each other through the relay: a room-visible message with a
`p` mention wakes the target on the normal ACP path (D-071). Colleague
behavior is prompts and thread presentation, not protocol:

- Ask-back in-thread and wait for the callback mention.
- Visible handoff: "A → B: what / why / expected back" — never looks like
  the work is done.
- Wrong-role work is refused or escalated, not silently done.
- No bare acknowledgements.
- Room-visible only; no private agent-to-agent talk.

### Email and other bridges

Start with Gmail via MCP as a tool of one Hermes "EA" profile in an
`#inbox` channel. Sending is always founder-approved through user-input
(46040). If mail must be visible to every agent and on mobile, move to a
bridge (inbound push → events in `#inbox`; outbound = approved draft →
send). Not chosen: a native mail client inside Tauri.

### Company memory

The Wiki (30023 company, 30623 repo) and per-profile Hermes memory (30174).
A handbook is a wiki page injected into persona prompts, not a new store.

## Desktop and mobile

| Surface | Role |
| --- | --- |
| Desktop | Main office: channels, agents, Hermes hire, Project/worktree, deep work |
| Mobile (existing Flutter app) | Continue the same company on a phone: Need you, read threads |

One product story, not two (D-026). No React Native rewrite; no
requirement for mobile parity with desktop admin.

## Worktrees (facts, do not invent)

- Threads do not always create worktrees; there is no "always worktree"
  toggle.
- Isolated worktrees apply to owner-authored Project task threads with
  trusted workspace metadata; the path must be a valid git repo.
- Ordinary channels, DMs, non-Project work use the default harness cwd.
- Failure to provision fails closed. Details: D-018, `crates/buzz-acp`.

## Out of scope unless a decision supersedes

- Local-first LLM stack as a product bet
- Replacing the Buzz backend or rewriting the kernel
- React Native rewrite of mobile
- Full mobile parity with desktop agent/forge admin
- Treating HTML spikes as shipped law
- Decorative org charts without assignment rules
- Assuming the founder knows corporate management practice

## How agents work with the founder (D-027)

MUST: explain like a smart colleague; say when guessing about "real
company" practice; cite `path:line` for code facts; separate Buzz kernel
vs Crew product vs spike vs your idea; surface doc conflicts instead of
picking one; refuse or escalate mis-assigned work; put durable outcomes in
the room; keep changes small and on-contract.

MUST NOT: hide bad news behind optimistic copy; invent Hermes-only wire
protocols; claim Claude/Codex have Hermes profile memory; propose client
rewrites as the default fix; expand mobile scope without an ask.

When stuck: ask one concrete question, offer A (recommended) / B.

Before asking for Accept, the thread has four items: 3-line story,
2-minute try script, evidence, honest limit
([`templates/CLIENT-ACCEPTANCE.md`](templates/CLIENT-ACCEPTANCE.md)).

## Success checks

1. Hermes agents live in channels; handoffs are intentional.
2. Wrong-role work is refused or asked back — not done silently.
3. Away from desk, the founder can unblock Need you on mobile.
4. Results and questions appear in the shared thread log.
5. Upstream Buzz remains pullable.
6. The founder can open any thread in Focus and watch the agent think and
   act with no more delay than a native CLI.

## History

The 2026-07 "board as orchestrator" framing (Issues → Planned → Working →
Need Input → Done, Working cap of three) is superseded by D-037:
channel-first stands; there is no board. The original text is in
[`archive/VISION.md`](archive/VISION.md).
