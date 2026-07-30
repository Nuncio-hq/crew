# Phase 3 — Agent context and validation

## Overview

- Priority: high
- Status: complete
- Purpose: pass the relay-confirmed workspace to an explicitly mentioned agent.

## Files to add

- `desktop/src/features/projects/lib/project-channel-agent-context.ts`
- `desktop/src/features/projects/lib/project-local-workspace-runtime.ts`
- additive Node contract tests, an SSR CommonMark rendering contract, and a
  gated live-relay integration test under the Project test directory

## Existing file to modify

- `desktop/src/features/messages/ui/useMentionSendFlow.ts`

The integration edit calls one additive resolver after explicit agent mentions
are known and before the outgoing event is published.

## Behavior

1. Ordinary messages perform no Project query and remain byte-for-byte intact.
2. An explicit agent mention fetches current kind `30617` announcements.
3. Only a unique Project with matching canonical `buzz-channel` is eligible.
4. Linked path and repo coordinate are appended as machine context.
5. The context instructs the agent to use absolute paths and states that
   `session/new.cwd` is unchanged.
6. Duplicate channel bindings, invalid locations, or relay failure block the
   agent-targeted send and preserve the draft.
7. A relink is observed on the next send; no module-level path cache is used.
8. Owned Project announcements and deletions are fetched fresh and filtered by
   the current identity before channel matching.
9. Machine context uses an unused CommonMark reference definition. It is absent
   from rendered chat but remains present in raw copy/edit surfaces.

## Validation matrix

| Check | Expected |
| --- | --- |
| Original RED contracts | Failed for the intended missing modules |
| Focused unit suite | All workspace and review-regression contracts green |
| Typecheck and Biome | Green |
| Native picker spike | Pick, cancel, and relink in a real Tauri app |
| Focused integration | Link, relink, relay reject, and cold reload |
| Visible message | Machine context is not rendered as manager chat text |
| Relay inspection | One canonical local tag; identity and other tags intact |
| Provider spike | Codex, Claude, Cursor, and Devin use absolute path; cwd unchanged |

## Availability limitation

Without a Rust filesystem adapter, the app cannot proactively stat an arbitrary
relay path after restart. The UI therefore distinguishes:

- `Selected now`: the native picker confirmed a directory at selection time;
- `Not locally verified`: cold-reloaded path has not yet been used;
- relay metadata that is invalid or cannot be loaded.

Do not label `Not locally verified` as available.
Missing or permission-denied paths surface in the agent/tool result at use time;
Crew does not persist a separate `Unavailable` UI state in this slice.

## Verification commands

```text
cd desktop
pnpm test
pnpm check
pnpm typecheck
pnpm build
```

Then run the real local relay and provider matrix. Do not report targeted
tests as the full repository gate.

## Accepted limits

- Relay lookup failure blocks every explicit-agent send because the app cannot
  query a multi-character `buzz-channel` tag before it knows whether the
  channel is a Project channel.
- Full removal of machine context from raw Copy/Edit requires a later timeline
  projection seam. Rendered manager chat remains clean in this slice.
- Use-time unavailability reporting is accepted for this no-Rust slice.
- There is no automated Playwright test for the panel and confirmation flow.
  Pure UI policy, SSR rendering, native-picker spike, and live-relay boundaries
  are verified separately to preserve the existing-file diff budget.
