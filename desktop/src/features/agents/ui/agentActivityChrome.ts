/**
 * Closed set of app-authored (chrome) strings for agent activity UI.
 * Content written by agents (checklist steps, questions) is never translated
 * here — keep that verbatim from the agent. Centralizing chrome makes a later
 * i18n pass a single-file change.
 */
export const AGENT_ACTIVITY_CHROME = {
  isWorking: "is working",
  agentsWorking: (count: number) =>
    count === 1 ? "1 agent working" : `${count} agents working`,
  agentsWorkingLabel: "Agents working",
  viewActivity: "View activity",
  stop: "Stop",
  seemsStuck: "seems stuck",
  workingFallback: "Working",
} as const;

/** Hide the live activity line until the turn has been alive this long. */
export const ACTIVITY_SILENCE_MS = 3_000;

/** Treat a turn as stuck when no new frame arrives for this long. */
export const ACTIVITY_STUCK_MS = 90_000;
