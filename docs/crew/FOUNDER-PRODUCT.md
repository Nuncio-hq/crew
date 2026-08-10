# Founder product north star (NuncioCrew)

- **Audience:** humans and agents implementing Crew on top of Buzz
- **Status:** Accepted direction (founder-locked 2026-08-10)
- **Decisions:** D-025, D-026, D-027 in [`DECISIONS.md`](DECISIONS.md)
  (D-024 on main is Hermes trusted/owner-only/local — related, separate)
- **Not this file:** day-to-day slice status → [`STATE.md`](STATE.md);
  Hermes runbook → [`HERMES.md`](HERMES.md); fork naming → [`IDENTITY.md`](IDENTITY.md)

## When to read

Read this **before** planning any product UX, agent runtime, mobile, or
“company / office” feature. If your plan conflicts with this file, stop and
ask the founder — do not silently prefer a spike, roadmap draft, or upstream
Buzz surface.

## One sentence

Crew is a **company on the founder’s machine**: people and agents share
**Buzz rooms** (channels/threads); **Hermes is the default employee**; other
ACP engines plug in through **existing Buzz contracts**; mobile continues
work away from the desk; the founder is the only real decision-maker.

## Plain language: what “company” means here

The founder has **not** worked as a company manager. Do **not** assume MBA
process, org politics, or “how real companies run.”

Here, “company” means only four practical things:

| Everyday need | In Crew / Buzz |
| ------------- | -------------- |
| A place to talk and leave a record | Channel + thread (Nostr events on the relay) |
| Workers who can do tasks | Agents (default: Hermes profiles) |
| Give the right work to the right worker | Mentions + (later) role rules — never silent mis-assign |
| Know when work needs a human | Need you / user-input + clear reports in the thread |

**The founder** states intent, answers Need you, and accepts or rejects
outcomes. **Agents** investigate, draft, code, report, and hand off — like
colleagues who must explain themselves simply.

## Platform choice (locked)

| Choice | Detail |
| ------ | ------ |
| **Keep Buzz backend** | Relay, Nostr identity, channels, ACP harness, event log |
| **Thin fork** | Prefer additive Crew files; keep fetching / syncing upstream |
| **Build on top** | Product defaults and UX live in Crew; do not replace the kernel |
| **Until** | A deliberate platform change is recorded as a new decision |

See D-001 (thin fork) and D-025 (build on Buzz contracts).

## Layers (what we care about)

| Layer | Meaning | Stance |
| ----- | ------- | ------ |
| **L0 Sovereignty** | Not locked to one vendor forever | **Anti-lock-in is enough.** Multi-engine via ACP. **No local-AI investment** (no Ollama-first path). |
| **L1 Org** | Agents as colleagues; handoff with roles | Hermes-first; roles must constrain assignment (see below). |
| **L2 Business** | Non-code work, questions, drafts | Same channels; no separate “Office” entity required day-one. |
| **L3 Build** | Normal app/mobile development | Agents + Project/worktree help ship real apps. |
| **L4 Evidence** | Before/after, verify work | Desired; ship on the **thread log** when prioritized — not a new platform. |
| **L5 Machine-as-cloud** | Self-hosted relay + agents on this machine | Core Buzz value; keep it. |

## Hermes-first, Buzz contracts always

### Mental model

```text
Buzz contracts (already exist)
  Nostr identity, channel, thread, mention, ACP session/turn, publish back
        │
        │  implement / optimize
        ▼
  Hermes (default employee)
  profile = person-like: memory, skills, credentials per profile
        │
        │  same contracts, thinner “person” model
        ▼
  Other ACP engines (Claude, Codex, …)
  session-oriented; do NOT fake Hermes profile memory
```

### Rules for implementers

1. **Optimize product paths for Hermes** (hire, spawn, docs, defaults, UX).
2. **Use existing Buzz/ACP/Nostr contracts first.** Do not invent a parallel
   Hermes-only protocol for room membership, assignment, or results.
3. **Extend contracts only when generic Buzz is truly insufficient** and
   record why in `DECISIONS.md`. Prefer extensions that other ACP engines can
   ignore safely.
4. **Never pretend** non-Hermes engines have Hermes profile memory or
   profile-owned model config. UI and docs must say what is Hermes-only.
5. Hermes detail and CLI: [`HERMES.md`](HERMES.md) and feature
   [`features/0001-hermes-first-class-runtime.md`](features/0001-hermes-first-class-runtime.md).

### Roles and assignment (intent)

Org chart / roles are valuable **only if** they change behavior:

- **Good:** work of type X goes to an agent allowed to do X; wrong-role
  handoff is refused or escalated to the founder.
- **Bad:** decorative titles while CTO assigns API work to “marketing,” or
  backend agents silently ship unrelated UI.

Day-one enforcement may be soft (profile prompts + honest refusal). Hard
harness policy can come later. Mis-assignment must never look like success.

## Desktop and mobile (one product story)

Do not maintain two product myths (“Crew mobile” vs “some other mobile”).

| Surface | Role |
| ------- | ---- |
| **Desktop** | Main office: channels, agents, Hermes hire, Project/worktree, deep work |
| **Mobile app** | Continue the same company on a phone: Need you, read threads, keep work moving when away from the desk |

- The **first mobile app that matters** is the one that connects to this
  workspace and continues work.
- Prefer improving the **existing Flutter client** over a React Native rewrite
  for “easier TestFlight.” Fix distribution/dev loop if install pain is the
  issue.
- Mobile need **not** mean desktop parity (no requirement for full Projects,
  agent admin, or forge on phone in early slices).

## Channel vs “Office”

- **Channel** = real primitive (protocol + shipping UI).
- **Office** = product *lens* in spikes (default home = Slack-like channel).
  There is no separate shipped “Office” object. Do not invent one unless a
  decision says so.

## Worktrees (facts agents must not invent)

- Threads do **not** always create worktrees.
- There is **no** user toggle “always worktree.”
- Isolated git worktrees apply to **owner-authored Project task threads** with
  trusted workspace metadata; path must be a **valid git repo**.
- Ordinary channels, DMs, non-Project work: agent uses default harness cwd.
- Failure to provision fails closed (no silent fallback to source tree).

Details: D-018, `crates/buzz-acp` thread workspace code, [`STATE.md`](STATE.md).

## In scope / out of scope (founder product)

### In

- Buzz kernel + continuous upstream fetch/sync
- Channel-first multi-agent room
- Hermes as default provider/runtime experience
- Generic Buzz contracts for all ACP engines
- Role-safe handoff direction
- Desktop as primary control surface
- Mobile app for Need you + continuity
- Business and code work in the same rooms
- Honest, simple communication (see [`AGENT-WORKING-AGREEMENT.md`](AGENT-WORKING-AGREEMENT.md))

### Out (unless a new decision supersedes)

- Local-first LLM stack as a product bet
- Replacing Buzz backend / rewriting the monorepo kernel
- React Native rewrite of mobile “for Expo”
- Full mobile parity with desktop agent/forge admin
- Treating mission-control / personal-office **spikes** as shipped law
  without a decision + implementation
- Decorative org charts without assignment rules
- Assuming the founder knows corporate management practice

## Success checks (product, not vanity)

A direction is “working” when:

1. Hermes agents live in channels; handoffs are intentional.
2. Wrong-role work is refused or asked back — not done silently.
3. Away from desk, the founder can unblock Need you on **mobile** and read
   the thread.
4. Results and questions appear in the **shared thread log**, not only in
   private tool transcripts.
5. Upstream Buzz remains pullable (thin-fork discipline).

## Spikes and roadmaps

HTML spikes under `docs/crew/spikes/` (mission-control, personal-office) are
**discussion instruments**. They may inspire UI. They are **not** automatic
implementation orders. Prefer this file + `DECISIONS.md` over spike chrome.

## Related reading

| Doc | Why |
| --- | --- |
| [`AGENT-WORKING-AGREEMENT.md`](AGENT-WORKING-AGREEMENT.md) | How agents must talk and decide with this founder |
| [`VISION.md`](VISION.md) | Earlier mission-control framing (board lifecycle) — reconcile conflicts with this file by asking |
| [`HERMES.md`](HERMES.md) | Operational Hermes |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Crew technical boundaries |
| [`UPSTREAM-SYNC.md`](UPSTREAM-SYNC.md) | How we stay a thin fork |
