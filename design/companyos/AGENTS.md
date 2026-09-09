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

## Stage 0 handoff (2026-09-09)

The conceptual Projects/Channels split is accepted as recorded in blueprint.js. Detailed Projects/Channels layout and interactions remain under founder discussion: preserve the current visual source until explicitly approved. The Stage 0 matrix owns accepted/proposed/blocked scope. Private reference captures remain local and are not public assets.
