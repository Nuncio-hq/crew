# Thread Worktree UI and Stable Release

## Goal

Ship a truthful Project-thread workspace surface without changing the normal
Slack-style composer, then publish a stable Crew update from `0.0.4`.

## Decisions

- One Project channel continues to host many independent threads.
- Sending a normal message with ordered agent mentions remains the only entry
  point; no prompt modal or focus button is added.
- Worktree state comes from ACP observer lifecycle events, not timers or
  prototype-only simulation.
- The UI shows the existing deterministic `buzz/<root-prefix>` branch and
  sibling `.buzz-worktrees` path. Semantic model naming is a later feature.
- Agent ordering comes from message tags/text; working and completed states
  come from active-turn telemetry and signed agent replies.
- App version `0.0.5` publishes under immutable Crew tag `crew-v0.0.5` so
  inherited Buzz tags cannot block the release.

## Phases

1. [Truthful workspace telemetry](phase-01-truthful-workspace-telemetry.md)
2. [Thread workflow UI](phase-02-thread-workflow-ui.md)
3. [Release and verification](phase-03-release-and-verification.md)

## Success

- Two Project threads can provision and report different worktrees at once.
- Thread UI shows preparing, ready, and error states from real lifecycle data.
- Existing thread chat, replies, mentions, and handoffs remain unchanged.
- Desktop E2E screenshots match the selected prototype direction.
- CI, independent tests, and code review pass.
- Stable `0.0.4` detects, installs, and relaunches into `0.0.5`.

## Unresolved Questions

None for this release. Semantic worktree naming and cleanup/GC remain separate
follow-up features.
