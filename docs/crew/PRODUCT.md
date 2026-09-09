# Crew — product

Crew (product name **NuncioCrew**) is the founder's CompanyOS: one app for
software delivery, clients, deadlines, communication, and business work with
agents as colleagues. Hermes is the primary employee runtime; Buzz provides
the shared coordination foundation. The founder sets direction and accepts
outcomes.

This is the product north star, updated from the founder's clarification on
2026-09-08 (D-076). Confirmed direction below is not a claim that every
capability ships today. Implementation status belongs in [`STATE.md`](STATE.md)
and boundaries in [`ARCHITECTURE.md`](ARCHITECTURE.md). Proposals and open
questions are explicitly separated. Rewrite this file when intent changes;
do not append brainstorm transcripts.

Read this before planning any product UX, agent runtime, mobile, or
"company" feature. If a plan conflicts with this file, stop and ask the
founder.

## The problem

The founder has a senior software engineering background and now also works
as an application specialist advising researchers on computing infrastructure.
He continues to build mobile and web products across personal, client, and
company projects. CompanyOS must support both technical and non-code work.

The friction is switching between agent apps, project tools, communications,
and execution environments while personally tracking who is doing what and
whether the result is correct. Opening one app with MCP integrations is a
useful starting point. The desired product also connects clients, deadlines,
email, marketing (including X growth and advertising), browser use, and
simulators to the work being done. These are confirmed needs, not an approved
integration order or a requirement to rebuild every external app natively.

## What "company" means here

Use the founder's technical experience without assuming familiarity with
company-management process. "Company" means practical delegation:

| Everyday need | In Crew / Buzz |
| --- | --- |
| A place to talk and leave a record | Channel + thread (Nostr events on the relay) |
| Workers who can do tasks | Agents (default: Hermes profiles) |
| Talk to the relevant department and have it dispatch work | Named agents + channel roles; department organization remains to be designed |
| Know when work needs a human | Need you / user-input + clear reports in the thread |

The founder can speak directly with Marketing, a CTO, or another relevant
department lead. That lead coordinates specialists and returns the result.
A single mandatory CoS contact is not the product requirement (D-076
supersedes that part of D-072). Department names are examples, not a fixed
roster or a new authority schema.

Vocabulary to use with the founder: room/channel, thread, employee/agent,
assign (= @mention), Need you, report, desk (desktop), phone (mobile).
Avoid consultant vocabulary and process diagrams the founder did not ask for.

## Platform (locked)

| Choice | Detail |
| --- | --- |
| Keep Buzz backend | Relay, Nostr identity, channels, ACP harness, event log |
| Company deployment | Use the founder's [chosen dev-server deployment](ARCHITECTURE.md#company-deployment); other configured communities remain supported |
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

The founder still likes Slack-style conversation and threads. The problem
is keeping ongoing work understandable as threads, projects, and agents
accumulate. CompanyOS must show what is queued, active, waiting for input,
ready for review, and complete without requiring the founder to open every
thread. These are user needs, not a finalized state machine. The founder selected the
sidebar and demonstrated Project/Wiki v0.9 flow in D-078; backend proof gates remain explicit.

Build this on Buzz's channels, threads, roles, mentions, and relay lifecycle.
The chosen queue, department, and integration designs must name the existing
seams they extend. A customer is not automatically a channel; a department
is not automatically a new roster database. Those mappings need design work.

### First priority: a complete coding delivery loop

Before implementing its user-facing experience, establish the shared visual
reference described below. Choosing coding delivery first does not skip
agreement on what the CompanyOS app will look like and how it will behave.

The founder selected this first on 2026-09-08:

1. Founder and agents brainstorm a project or change and agree on a plan,
   scope, and observable acceptance criteria.
2. The responsible lead breaks the plan into work, queues it, and delegates
   to suitable specialists without the founder manually relaying every step.
3. Agents implement, review, test, and verify the actual result, repairing
   failures within the agreed scope.
4. The lead returns a usable result with evidence and honest limitations.
   The founder judges acceptance; passing CI alone is not acceptance (D-070).

This is a delivery target, not a declaration that autonomous orchestration
already works end to end. Clients, email, deadlines, and marketing remain
part of the long-term scope. The first concrete coding project is still open.

### Autonomy within agreed work

After planning, agents should continue through the approved queue rather
than wait for a new prompt at every step. Assigned issue backlogs and agreed
testing or performance checks can drive work. Do not treat every idea or
discovered issue as permission to start a new project.

Agents ask for judgment when scope, priorities, or unresolved product choices
change; they own routine execution and verification. Recurring work, spending,
publication, and destructive actions still need the authority appropriate to
that task. This product direction grants no blanket operational permission.

### Verification is a core employee skill

The founder must be able to trust the result, not merely the agent's report
that tools ran successfully. Skills must encode how to establish correctness
for the kind of work being delegated. The lead owns a complete handoff;
delegating implementation does not delegate away responsibility for evidence.

Use evidence appropriate to the agreed outcome: exercise the user flow for
UI work, show comparable measurements for performance claims, and check
sources and requirements for non-code deliverables. State what was actually
tested, in which environment, what failed, and what remains unverified. Do
not turn screenshots into a universal proof requirement. Technical checks
are in [`TESTING.md`](TESTING.md); the founder's acceptance remains separate.

### Departments

The intended hierarchy is functional: the founder talks to a department,
its lead dispatches work, specialists collaborate, and the lead reports back.
Persistent Hermes profiles provide employee persona, memory, skills, and tools.
Crew owns company coordination and visibility; the runtime executes the work.

Use existing channel roles and named-agent calls (D-043/D-044/D-071) first.
A channel template is a candidate setup mechanism, not an approved complete
department implementation. D-069 still prohibits reviving the removed Org
roster and ORG-CHECK as a shortcut. Functional hierarchy does not by itself
authorize a new org-chart screen or officer protocol.

### Watching agents work: Office and Focus

The desired experience offers two levels of detail on the same work:

- **Office** — Slack-style: kickoff, receipts, evidence, questions,
  results. What a manager reads.
- **Focus** — inspect live agent output, tool activity, plans, and relevant
  browser / simulator / terminal context; steer or stop when necessary.

Display reasoning or tool details only when the runtime actually exposes
them; do not fabricate unavailable output. Keep everyday reporting concise
and deeper inspection accessible. Live progress should project existing message/thought/tool events without a separate agent turn to summarize each update. Channel roles remain channel-scoped (D-043); thread ownership or participation does not assign a new role.

These names describe the desired experience, not two newly approved routes.
D-065 superseded D-055's Workbench destination: current `/workbench` routes
redirect to Inbox or the channel thread. Further Focus work is tracked in
[#344](https://github.com/Nuncio-hq/crew/issues/344); its navigation and
historical-session access must be reconciled with D-065 before implementation.

### Workspace layout direction (D-078)

The founder selected a Codex-style layout on 2026-09-08:

- Sidebar top: Inbox, Agents, Workflows.
- Sidebar middle: Projects, each expanding to Wiki and its channels.
- Workspace menu: Browse channels, including shared/orphaned joined channels.
- Sidebar bottom: direct conversations with agents.
- Main area: the selected channel, thread, or direct conversation.

This direction supersedes D-066's prohibition on Projects in navigation. It
is not yet shipped. It does not restore a separate Workbench thread picker.
The accepted Project Channels/Workspace page uses existing Project/Repository
relationships. #361 resolves remaining legacy/zero/multiple-repository, exact
path and recovery semantics; the visible word "Project" must not redefine identity.

The maintained [CompanyOS blueprint](../../design/companyos/README.md) owns
the design reference: `design/companyos/index.html` explains each area and
its boundaries; `prototype.html` retains the interactive UI. Explanations live
in `src/design-contract.js`. Its `src/blueprint.js` supplies the review state
IDs, actions, expected results, and acceptance labels shown in the adjacent
Blueprint panel. The demonstrated Project/Wiki v0.9 flow is accepted per #344;
unproved backend contracts and unrelated proposals remain named in that matrix.
Prototype behavior is not runtime evidence.

### Shared visual reference before UI implementation

The founder wants the working method used in The13, HeardBack, and Didit:
one maintained HTML reference combining the product journey, screen images
or interactive prototypes, and adjacent notes about actions, rules, and open
decisions. It is a shared design and handoff surface, not merely an app demo.

Review the overall app journey, then refine individual screens and states
together. Keep review notes and scenario controls outside the simulated app.
Distinguish accepted design, proposals, static concepts, and working local
interactions; label simulated agents and external services. Include failure
and recovery states as well as the happy path, with steps and expected results.
An attractive screenshot alone does not approve behavior.

Update the same reference as decisions evolve; do not make a new prototype
for every task. Link accepted behavior to the living docs rather than creating
a second technical spec. Agents implement the agreed states and verify the
real app against them. See [`DEVELOPMENT-WORKFLOW.md`](DEVELOPMENT-WORKFLOW.md).

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

The confirmed need is to work with email and other services from the same
app, connected to clients and deadlines. Gmail via MCP on a Hermes profile,
an `#inbox` channel, and an inbound event bridge are candidates from earlier
brainstorming. The founder has not selected that architecture or the first
mail workflow. Do not silently implement a CRM, customer-as-channel mapping,
or native mail client from this list. Sending mail requires explicit authority.

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

## Work lifecycle and workspaces

A task is the intended outcome; a thread is its conversation; an agent
session is execution context; a worktree is one possible execution resource.
These distinctions guide design, not four approved new database entities.
Coding and non-code work must share an understandable lifecycle without
making every thread a Git checkout.

The founder needs completed work to stop cluttering active work and workspace
cleanup to be manageable. Acceptance, archival, session shutdown, and deleting
a checkout are separate actions. Their automation and retention rules remain
open. A one-off request to delete local worktrees is not a general policy to
discard unfinished work or erase task history.

Existing workspace contract (verify the affected path before changing it):

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

## Success checks for the first delivery loop

1. An agreed coding task progresses through delegation and verification
   without the founder repeatedly prompting each specialist.
2. The founder can see progress, blockers, and what needs a decision, then
   inspect the relevant execution detail without hunting across apps.
3. Evidence demonstrates the agreed behavior; failures and untested limits
   remain visible. The founder accepts a result rather than debugging for agents.
4. Completed work remains understandable while active work stays manageable;
   workspace cleanup follows an explicit policy.
5. Hermes is the optimized employee runtime; shared Buzz contracts remain
   replaceable by other ACP engines, and upstream remains pullable.

## Open questions for continued brainstorming

- Which real coding project or issue set should prove the first complete loop?
- What is the smallest department/lead setup, including cross-department work?
- How are queue priority, retries, recurring checks, and escalation presented?
- When should completed threads archive, sessions stop, and worktrees be removed?
- How should clients, deadlines, email, and marketing join that proven loop?

Answer these in the task and update the relevant section here when agreed.
They are not blockers to documenting the confirmed direction or permission
to invent the answers. Do not create a second CompanyOS spec.

## History

The 2026-07 "board as orchestrator" framing (Issues → Planned → Working →
Need Input → Done, Working cap of three) is superseded by D-037:
channel-first stands; there is no board. The original text is in
[`archive/VISION.md`](archive/VISION.md).

The blueprint's thread-lifecycle section now explores channel attention summaries,
thread-scoped tools, multiple linked PRs, Actions/CI/CD evidence, and explicit
result acceptance. These are review proposals, not shipped state semantics or a
new task protocol; see the maintained HTML reference before implementing.

### Stage 0 Project/Wiki contract (#344)

The founder approved the demonstrated Project/Wiki walkthrough and requested
implementation on 2026-09-09; D-078 records the exact message provenance.
Project name/breadcrumb opens Channels/Workspace; the chevron only expands.
Project Wiki supports Read/Search/Source, private existing-agent Ask/History,
independent generator settings and editable draft → explicit Start thread →
Back to Wiki. #361–#367 own backend implementation and proof; mocked source,
answers and timers are not evidence of those services.

Preserve company kind 30023 knowledge. The coordinator approved a Company Wiki
entry in the workspace menu opening existing `/wiki` / `WikiLibraryScreen`.
This approval covers the concrete compatibility entry, not a new founder claim.
#349 must verify the replacement before removing the old sole global Wiki affordance. It does not create another knowledge store.


The coordinator-approved G-THREAD-1 v3 contract in D-078 defines the thread tools:
one right pane, Context on first explicit opening, reachable Agent plans, and
bounded per-thread app-session selection. Narrow tools overlay mounted chat;
thread navigation closes presentation without stopping agents/resources.
Retained historical information remains readable; live controls require the
exact current generation. Browser/simulator activation is explicit and existing
governor hide/idle cleanup remains. Channel-mode compatibility and Workbench
redirects stay unchanged. This records implementation discretion, not a new
founder approval or proof that #354 is implemented.
