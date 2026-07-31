# Phase 02 — Thread Workflow UI

## Overview

Priority: high. Status: complete.

Port the selected prototype language into production components while retaining
the existing thread header, timeline, composer, accessibility, and responsive
panel behavior.

## UI

- A compact execution strip below the thread header.
- Preparing, ready, and error workspace cards above replies.
- Branch chip and accessible details popover with copy-path action.
- “Handoff in this thread” list using ordered agent mentions/references.
- Statuses: working from conversation-scoped active turns, done after that
  agent signs a reply, otherwise queued.

## Related Files

- `desktop/src/features/messages/ui/MessageThreadPanel.tsx`
- new small components under `desktop/src/features/messages/ui/`
- new pure derivation helper/tests under `desktop/src/features/messages/lib/`
- `desktop/src/features/agents/activeAgentTurnsStore.ts`
- `desktop/tests/e2e/`
- `desktop/src/testing/e2eBridge.ts`

## Implementation

1. Detect Project roots from the hidden `buzz://project-workspace` reference.
2. Derive ordered agent audience from existing `p` and `mention` tags.
3. Add a conversation-scoped active-turn snapshot.
4. Render workspace/workflow components only for Project thread roots.
5. Keep each new implementation file under 200 lines where practical.
6. Add deterministic E2E bridge frames and a two-thread visual test.

## Success Criteria

- Ordinary channels and ordinary threads render exactly as before.
- No “Prompt agents” control is added.
- The composer remains usable while setup is in progress.
- UI is keyboard accessible, zoom-safe, and uses existing Tailwind tokens.
- Two open roots display distinct branch/path/status values.

## Risks

- Reference-only agents must remain visible even though they do not receive the
  first notification.
- Active turns are keyed by deterministic conversation UUID, not root ID; the
  workspace event supplies the join.

## Unresolved Questions

None.
