# Prototype Instructions

Run the local server yourself and open the preview in the browser available to this environment. Do not give the user server-start instructions when you can run it.

Before making substantial visual changes, use the Product Design plugin's `get-context` skill when the visual source is unclear or no longer matches the current goal. When the user gives durable prototype-specific design feedback, preferences, or decisions, record them in `AGENTS.md`.

When implementing from a selected generated mock, treat that image as the source of truth for layout, component anatomy, density, spacing, color, typography, visible content, and hierarchy.

Build app UI in `src/`. Keep `.openai/hosting.json`, `worker/index.js`, `scripts/prepare-sites-build.mjs`, and `tests/sites-worker.test.mjs` intact so the same local prototype can be handed to Sites. Before a Sites handoff, run `npm run build` and `npm run test:sites`; the build must leave `dist/client/index.html`, `dist/server/index.js`, and `dist/.openai/hosting.json`.

## Executable design contract (founder feedback, 2026-09-09)

Every proposed UI behavior must name its existing backend source, required new wiring, failure/access states, and real verification still needed. A film or mock is not feasibility evidence. Keep this mapping in the existing design document and living architecture docs.

Use direct observer message/thought/tool events for live activity; do not require another agent turn to summarize each update. Show thoughts only when the runtime exposes them. Preserve owner-only telemetry visibility and show missing/disconnected states honestly.

Roles are channel-scoped owner-signed canvas assignments (D-043/D-044). Thread ownership and participation are separate; neither silently creates or changes a role or capability. Do not infer role from provider, task title, or being the thread owner.

Include Assign roles on each channel, including empty channels. Save one channel-scoped assignment snapshot; all thread participants inherit it. Keep thread ownership, channel membership and capability grants separate.

## Management controls and Git evidence (2026-09-09)

Expose Add role and Add agent / Edit, not only editing existing fixtures. A role definition can be unassigned. Hermes agent configuration selects a profile only; models are configured in that profile. Codex/Claude can set a runtime model. Keep stable identity when display names change. Show branch, worktree and local +/- where evidence exists, with separate PR diff counts. Use green Open, purple Merged, red Closed and gray Draft for PR lifecycle; CI must not control PR lifecycle color.

## Channel contact point (2026-09-09)

Assign roles includes one optional Channel contact point selected from channel agent members. Human messages with no explicit mention route to that agent, including thread replies, without changing thread ownership or roles. Explicit mentions take priority; agent-authored messages must not trigger this fallback. None keeps mention-only behavior. Save roles and contact together; Cancel discards both. Show the selection in the channel and composer. Treat this as proposed backend work until verified through the real relay.

## Agent deletion, recap settings and plans (2026-09-09)

Include Delete agent with an explicit effect preview and cancel path. Keep conversation authors/history; remove deleted agents from live pickers, DM navigation, role holders and contact points, preserving unassigned role definitions. Do not delete runtimes, profiles or worktrees as a side effect.

Place Settings in the bottom user row beside the avatar/name, replacing Personal; do not add a separate sidebar row (founder feedback, 2026-09-09). Settings provides user-facing recap runtime/model selection based on runtime discovery; Hermes selects a profile. Default recap to Off with an explicit Generate action in Context. Keep generated recap distinct from structured thread fields, direct Activity and runtime compaction/handover. Direct events/plans must not trigger another summarizer turn.

Expose each participant's own ACP plan or structured todo snapshot in Thread tools. Preserve source, statuses, thread/agent identity and missing/disconnected states. Plans do not establish completion or acceptance. Keep all new paths explicitly simulated until live backend verification.

## Project page review (founder request, 2026-09-09)

Design a lightweight Project page with Channels and Workspace for founder review. Project names and the project breadcrumb open this page; a separate arrow expands channels. Shared channels remain available through the workspace menu. Add Project includes name, optional local folder with Git detection, and a new or existing main channel. Reuse the current visual language. Folder selection, detection and writes stay explicitly simulated. The demonstrated Project/Wiki v0.9 flow was subsequently accepted; use the approval provenance below. Backend assumptions still require their named proofs.

## Workspace channels refinement (2026-09-09, superseded navigation below)

Channels within Workspace is a disclosure, not a landing page. Show standalone channels directly underneath and open their conversations in one click. Hide the disclosure when no standalone channels remain; Browse channels in the workspace menu stays available, including for common channels linked into Projects. Project pages retain Channels and Workspace. Choose a folder uses the existing native macOS picker followed by Git detection in production; the browser preview remains explicitly simulated.


## Project Wiki refinement (founder request, 2026-09-09)

Latest prototype navigation is Workspace: Inbox, Agents, Workflows; each expanded Project contains Wiki followed by channels. Remove the global Wiki and Channels rows. Browse channels remains in the workspace menu. Project names still open the Channels/Workspace overview. This supersedes the earlier Channels-disclosure placement for the prototype; it does not delete any channel or company handbook content.

Build one large Wiki surface inspired by the selected Devin Wiki reference, retaining Crew typography/colors: Read has TOC and article; Ask replaces the article; a source panel hides TOC. Remember the page/scroll and private questions per project/repository in the preview. One repository is automatic; only multiple repositories need a selector. Source revision/branch are display metadata, not separate branch wikis.

Use an existing available project agent for Ask. Keep questions private until an explicit editable task draft is started in a selected channel with a selected member. Preserve source references and a Back to Wiki link. Generation is a temporary runtime/profile session independent of Recap and managed agents. Include empty/missing workspace, updating/cancel, failed/retry, stale, unavailable agent and failed-answer states. Do not represent fixture answers or timers as live indexing or agent execution. Real full-content search, QA, citations/history and thread dispatch must bind the existing Wiki and channel models in production.

## Stage 0 v0.9 approval and source handoff (2026-09-09)

Founder messages after the Wiki walkthrough, recorded in #344: “tốt rồim, tạo issues mới đi” and “wiki và cả project nhé nếu chưa tạo”, followed by explicit implementation and real-data evidence instructions. These supersede earlier pending-review Project/Wiki wording. Implement the demonstrated accepted flow without asking for the same permission again; keep unproved backend/privacy/protocol choices under #361–#367 gates.

`src/blueprint.js` owns the accepted/proposed/blocked matrix. Workspace menu → Company Wiki is a coordinator-approved compatibility entry for the existing production `/wiki` library and kind 30023 company knowledge. Its local sample preview is not a new production library, and the coordinator approved this concrete menu/sample/back-action diff on 2026-09-09; production removal still waits for #349 replacement verification. Private captures remain excluded. Group related checks against one unchanged build and retain issue-to-case evidence mapping.
