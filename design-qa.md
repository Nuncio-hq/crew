# Project thread worktree UI — design QA

- Reference: `nuncio-thread-worktree-prototype/agent-prompt-flow.png`
- Implementation capture: `desktop/test-results/thread-worktree/02-full-project-thread.png`
- Viewport: 1200 × 750
- State: three-agent Project thread with a verified shared worktree and one active handoff step

## Visual checks

- Preserves the existing Slack-style channel timeline, thread side panel, and standard reply composer.
- Keeps the user's prompt as the root message; no redundant “Prompt agents” action.
- Places workspace identity, readiness, and handoff order directly below the root message.
- Uses the existing type scale, borders, spacing, avatars, theme colors, and icon set.
- Branch detail remains compact while the full verified path is available from the branch chip.
- Narrow thread widths wrap readiness copy without clipping controls or agent status.

## Interaction checks

- Branch chip opens verified worktree details and copies the path.
- Pending, ready, and safe error states are driven by backend observer events.
- Two thread roots in one channel retain distinct branch/worktree projections.

final result: passed
