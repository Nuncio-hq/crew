# CompanyOS design blueprint

This is Crew's **one maintained visual and interaction reference**, starting
with the founder-selected Codex-style workspace layout (2026-09-08, D-078).
It is an independent prototype. It does not modify the installed app or connect
to its relay. Keep evolving this folder; do not create a new demo for each issue.

## Open and run

From this folder, with the repository's Hermit environment active:

```sh
npm ci
npm run dev -- --host 127.0.0.1 --port 4320 --strictPort
```

Open `http://127.0.0.1:4320/` for the design document and
`http://127.0.0.1:4320/prototype.html` for the UI. `index.html` is the design document. `prototype.html` is the interactive UI.
Deep-link a review state with `prototype.html?film=off&state=needs-you`, for example. Wiki states also accept `project=NuncioCrew`.
Build validation: `npm run build`. The compiled app is `dist/client/`.
This handoff is local-only; no Sites deployment is authorized. Original private captures are excluded.

## Authority and ownership

| What                                                             | Authoritative location                                                                                    |
| ---------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| Accepted product intent                                          | [PRODUCT.md](../../docs/crew/PRODUCT.md)                                                                  |
| Why the direction changed                                        | [DECISIONS.md](../../docs/crew/DECISIONS.md), D-078                                                       |
| Selected visual input                                            | Private Codex capture — local only, excluded                                                     |
| Current app comparison                                           | Private Crew capture — local only, excluded                                            |
| Review state IDs, actions, expected behavior, status, Buzz seams | [src/blueprint.js](src/blueprint.js), rendered in **Blueprint & notes**                                   |
| Visual implementation                                            | `src/App.jsx`, focused components, `src/styles.css`                                                       |
| Most recent visual verification                                  | [design-qa.md](design-qa.md)                                                                              |
| How agents implement accepted states                             | [DEVELOPMENT-WORKFLOW.md](../../docs/crew/DEVELOPMENT-WORKFLOW.md#shared-reference-for-companyos-ui-work) |

The first artifact is justified because Markdown cannot provide an interactive
layout and state comparison. This README owns artifact usage only; it is not
a second product specification. `design-qa.md` is the required local design QA
record, updated in place rather than multiplied per iteration.

## What is accepted, and what is not

The founder accepted the demonstrated Project/Wiki v0.9 flow on 2026-09-09,
then requested implementation and real-data evidence. Approval provenance in
[#344](https://github.com/Nuncio-hq/crew/issues/344): “tốt rồim, tạo issues mới đi”
and “wiki và cả project nhé nếu chưa tạo”. Earlier pending-review notes below
are historical, not a requirement to obtain that same approval again.

Workspace top is Inbox, Agents, Workflows. Wiki is inside each Project; there
are no global Wiki/Channels rows. Browse channels stays in the workspace menu,
including joined channels with missing/inaccessible Project metadata. Project
name/breadcrumb opens Channels/Workspace; chevron only expands/collapses. Keep
existing home/related/repository channel relationships and many-to-many bindings.
Kind 30621 is a Project; kind 30617 is a Repository. Sample name-based state is
not a production identity model.

`src/blueprint.js` exports the authoritative `handoffContract` matrix, rendered
in `index.html#authority`. Accepted flow is distinct from unproved backend
semantics: G-PROJECT, G-PUB, G-DURABLE, G-GEN and G-ASK-PROOF remain explicit,
alongside the existing thread/recap/contact/acceptance gates. A working sample
button is not production evidence. The **Company Wiki** workspace-menu entry
is a coordinator-approved compatibility choice for existing `/wiki` /
`WikiLibraryScreen` / kind 30023 pages. #349 must verify the production replacement
before the old sole global entry is removed. This is not a new founder claim. The sample library does not query real pages.

## Working rules for implementation agents

1. Read the Crew product, decisions, fork guidance, and applicable tests.
2. Choose the exact state ID and accepted contract in `src/blueprint.js`; ask only about genuinely unresolved choices.
3. Follow recorded approval and update the affected living-doc contract and this same reference.
   Do not mark other states accepted by inference.
4. Map the state to the named Buzz seam. Reuse existing relay identity and lifecycle.
5. Implement in production separately. Do not copy the prototype's in-memory
   state model, example PR output, timers, or text into authoritative runtime code.
6. Exercise the real flow against that state; follow D-077 for real-data testing.
   Keep prototype visual QA, mock tests, and actual agent verification distinct.
7. Report the state ID, commit, evidence and limits. Visual approval does not
   authorize PR merge, release, or live-agent execution.

## Prototype boundaries

- Sample conversations, directory, documents, PR diff, browser page, terminal output.
- Messages and drafts are held in memory. Reload/reset clears them. No real sends.
- DM reply is a short timer simulation. Switching views cannot deliver to another DM.
- Reconnect, choice confirmation, Stop/Steer, and workflow pause are local examples.
- Attach context is explicitly deferred; it does not upload files.
- Global search covers the sample workspace. Project Wiki search searches the full text of its sample pages; it does not query the daily relay.
- Escape closes review/search or returns from the thread to the channel.
- Keyboard arrows on the divider resize the two panes. Narrow windows show tools
  as a dismissible foreground pane so conversation text is not squeezed.
- Blueprint notes and scenario controls are outside the simulated app.

## Assets

Inter is bundled locally under the SIL Open Font License (see `public/fonts/`).
Lucide provides the thin outline UI icons, matching the supplied reference and
Crew's existing icon vocabulary. The supplied references are retained locally;
no generated raster imagery or external image service is needed for this UI.

## Document versus interactive UI

`index.html` renders `src/document-main.jsx` and explains each workspace area,
actions, boundaries, open decisions, vocabulary and agent handoff.
`src/design-contract.js` owns those explanations. Scenario details render from
`src/blueprint.js` in both pages; do not duplicate that state contract.
`prototype.html` retains the UI with its own entry and stylesheet. Its Design
document link returns here. Both HTML entries are included in the static build.

## Multi-thread review (v0.2)

Start at `prototype.html?state=channel`. Seven sample threads share one local
state projection with Inbox. `src/thread-model.js` owns sample thread and PR
data; `src/blueprint.js` remains the scenario contract. The lifecycle explanation
is in `index.html#thread-lifecycle`. Thread tools close on channel navigation.
Try Accept result on the delivery checklist, then channel Done and Inbox; try
Reopen, PR switching, Actions job logs and switching between thread drafts.
The internal legacy scenario status `done` means Ready for review; `completed`
means accepted outcome. These are fixture keys, not proposed protocol values.
All run IDs, revisions, checks and acceptance are examples. Actions execution,
provider integration and durable receipts are not implemented.

## Card hierarchy and motion (v0.3)

Thread cards now have separate surfaces, attention accents, per-PR badges and
expandable roles. The original per-work role fixtures were incorrect and are superseded by v0.5 channel canvas assignments.
Play activity demo is an opt-in, bounded sequence owned by App, outside the app
chrome. It changes only the sample checklist and never auto-accepts; End demo
restores that fixture. Reduced motion retains all textual states with no motion.
The existing lifecycle section in index.html owns the design explanation.

## Autonomous workspace film (v0.4)

`prototype.html` now autoplays a 132-second story. `src/film.js` owns its pure
absolute-time projection; `FilmPlayer.jsx` owns transport, story cursor and
campaign artifact presentation. Existing sidebar, conversation, cards and tools
render the projected state. No second app or external integration was created.

Pause, seek, chapters, 0.5x/1x/2x/4x and restart operate on the same clock. Seeking
back reconstructs messages, drafts, PR revisions and receipts without future
state leaking backward. The film stops at the end. Explore app switches to the
current snapshot; scenario deep links must include `film=off`. The old short
activity demo is superseded. Story acceptance is fictional and cannot authorize
real actions. Marketing and customer channels are also available in exploration.

SVG avatar sources and license: [public/brands/README.md](public/brands/README.md).

## Backend-grounded activity and channel roles (v0.5)

`index.html#feasibility` records code-inspected seams, missing integration work and limits. `activity-model.js` supplies authored observer-shaped examples for both the compact card and full Activity view. The manual preview runs one bounded 24-second sequence; Reset/scenario selection replays it. Film seeks reconstruct activity at the same absolute time. These are still simulations, with no extra summarizer call and no live relay attached.

`channelTeam` supplies sample channel canvas assignments; `threadTeam` resolves participants against that channel roster, independent of the thread owner. Unknown channels have no inferred roles. The production source is D-043 owner-signed canvas data, not this fixture or provider logos.

Production code inspected: `crates/buzz-core/src/crew_role.rs`, `crates/buzz-core/src/observer.rs`, `crates/buzz-acp/src/acp.rs`, `desktop/src-tauri/src/commands/canvas.rs`, `desktop/src/features/agents/ui/agentSessionTranscript.ts`, and `desktop/src/features/messages/ui/conversationActivityHeadline.ts`. Observer frames are owner-scoped, encrypted and ephemeral. Do not promise all channel members can see them or that reconnect recovers every past frame. A live multi-engine verification remains required before shipping.

## Management and Git evidence (v0.6)

`ChannelRoles.jsx` now edits role definitions and holders. Add role creates a definition that can remain Unassigned. Selecting an agent moves its single assignment within that channel; several agents can share a role. Blank/duplicate names and missing definitions block Save. Cancel/Escape discard drafts. The roster shows unassigned definitions as well as holders.

`AgentDirectory.jsx` owns sample stable IDs and channel memberships. Add agent / Edit in `AgentEditor.jsx` configure name and runtime. Hermes selects only an example profile; Codex/Claude accept an optional model ID (blank inherits the runtime default). Add can select channels; the agent then appears in their role pickers. Runtime model access and profiles are not queried. Saving does not start agents or alter profiles. Reset/reload restores the examples.

`ThreadWorkspace.jsx` shows branch, checkout name/path, and local +/- against HEAD only when a checkout is attached. Removed checkouts are labeled; non-code work has none. Each PR has its own head/base and PR +/- in Tools. PR lifecycle uses GitHub colors independently of CI: green Open, purple Merged, red Closed, gray Draft. The accepted unread thread includes an old Closed PR so the distinction is reviewable. Source: [Primer StateLabel](https://primer.style/product/components/state-label/).

Backend mapping and gaps live in `index.html#management` / `#feasibility`: reuse channel canvas definitions/assignments, managed-agent create/update and Hermes profile discovery, worktree registry/detail and thread GitHub/forge data. Creation plus channel joins needs acknowledgement and partial-failure recovery; the prototype's single local save is not a claim of a backend transaction. Canvas writes must preserve prose, routing/capabilities and handle conflicts. No production code or real agent configuration is changed.

## Channel contact point (v0.7)

Assign roles includes an optional channel member who replies to human messages without an explicit @mention, including replies inside channel threads. Mentions select their recipients without also calling the contact. None keeps mention-only behavior. Roles and contact save as one local snapshot; Cancel/Escape discard both. The channel and composer display the selection. Messages and agent responses remain simulated. Agent messages never re-enter the sample fallback; unavailable contacts do not silently switch to another agent.

`contact-routing.js` drives the prototype send path. Plain-text display-name matching is demo-only; production needs structured mention targets and verified author identities. Backend mapping and missing work are in `index.html#management` and the existing architecture integration boundary. No real canvas, ACP subscription or agent configuration is modified.

## Agent deletion, recap settings and plans (v0.8)

Agents now includes Delete with an effect preview and Cancel. Sample identity tombstones preserve author names and conversations while removing the agent from active directory/DM/pickers, channel holders and contact points. Role definitions remain. Removing a working owner stops its sample thread state; no real process, runtime/profile or worktree is changed.

Settings lives beside the avatar/name in the bottom user row, replacing Personal without an extra sidebar row. It has a separate recap runtime selector: Off by default, Hermes profile, or Codex/Claude optional model. Available runtimes are a discovery fixture, independent of agent instances. Context includes Generate/Regenerate recap and a settings shortcut; output is authored sample text with the selected configuration label, not model output. Changing settings does not relabel old recap provenance. The sample stale check covers thread fields, message count and agent identities; production needs an authoritative event watermark.

Thread tools now has Agent plans, with one reported ACP plan/structured todo snapshot per participant. The working sample advances once with the existing activity clock; missing plans and disconnected history stay explicit. This is direct event projection, not a recap. Existing parser/projection and deletion backend seams, and new recap wiring requirements, are documented in `index.html#management` and Crew architecture.

## Backend/frontend and GitHub audit (2026-09-09)

The historical pre-roadmap audit is in `index.html#feasibility`, backed by `src/design-contract.js`; backlog proposals are in `#backlog-audit`. It compares production code, prototype interactions and GitHub issue history. Snapshot: 5 open issues, 134 closed issues, 1 open PR (#347). Recommendation: rewrite #344, add three coherent issues (channel policy/contact point, runtime recap, isolated staging/acceptance), and retain distinct runtime/CI bugs. No GitHub changes were made. Focused production tests: 126 passed with no skips; no live engine or staging verification.

## Project page review iteration (2026-09-09)

Open `prototype.html?film=off&state=project` for the lightweight Channels / Workspace page and `state=channels` to open a general workspace conversation directly. The superseding v0.9 sidebar has no global Channels row or landing page. Browse channels remains available from the workspace name menu, including when all shared channels are linked into Projects. Click a project name or channel breadcrumb to open the Project page; the separate arrow expands its channels. Use the Projects `+` to review Add Project, including optional folder, sample Git/folder/unavailable results, and new/existing main channel. Add channel and Manage workspace also work in local sample state. Cancel/Escape discard drafts; Reset restores all fixtures. No native folder selection, Git mutation or relay write occurs. The demonstrated flow is accepted in #344; unproved backend contracts remain gated. The interaction contract is in `src/blueprint.js` and the existing design document, not a new spec.


## Project Wiki review

Open `prototype.html?film=off&state=wiki&project=NuncioCrew` for the current read → ask → source → task flow. Review scenarios include `wiki-empty`, `wiki-missing`, `wiki-updating`, `wiki-failed`, `wiki-stale`, `wiki-unavailable` and `wiki-answer-failed`. Sidebar Wiki belongs to each project; Browse channels remains in the workspace menu.

Six curated pages and seven source excerpts use Crew revision `8278d2b14cd04e17c1010d819c9fdd20eaefff9a`. The source excerpts are real captured code; answers are deterministic examples. Generator progress, private history, draft storage and new threads stay in this browser session. Reset/reload clears them. No runtime is started and no real task is posted. The existing design document owns the backend mapping and remaining verification work.

## Stage 0 v0.9 source handoff (#344)

Refreshed from founder checkout HEAD `a61590f2e626fe9e2dda3b405f588b0bd09e02b0`
with the reviewed v0.9 source and dirty living-doc hunks. The 74-file source/dirty
doc fingerprint matched before and after copying; aggregate SHA-256:
`e77b11cbffab03b1c39f56c9d65adc82a467dfe9d5beacb314fdfdcc96810cca`.
The Git commit carrying this handoff is the portable source revision. Downstream
issues pin that immutable commit after review/merge, not a localhost URL.

Public source excludes the two original private captures, dependencies, dist,
`.qa`, credentials and runtime state. Links use the safe mock reference instead.
Stage 0 changes source/docs only. PR #347 topology policy is inherited from
merged main `b90bbbf828e77e0d0d8e0b78bd86d8c6860fd2f9`, not reintroduced as
Stage 0 changes. Unrelated dirty HERMES/STATE/templates are not imported. Only PRODUCT and DECISIONS changed
among the previously reviewed docs since the first v0.8 snapshot.

Current delivery ownership is #344, #348–#357 and #361–#367. Related source
checks share one unchanged build with an issue-to-case map in existing
`design-qa.md`. Reference screenshots posted to #344 are explicitly simulated;
they cannot satisfy another issue's real installed-runtime evidence gate.
The roadmap stays open. No merge, release or production acceptance is implied.
