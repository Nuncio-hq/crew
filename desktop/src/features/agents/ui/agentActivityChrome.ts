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
  /** Counts runs, not agents: one agent can hold several live runs. */
  runsWorking: (count: number) => (count === 1 ? "1 run" : `${count} runs`),
  viewActivity: "View activity",
  openActivity: "Open Activity",
  /** Thread strip when no exact run identity is known: agent-scoped copy. */
  agentWorkingHint: (name: string) => `${name} is working — steer if stuck`,
  /** Agent-scoped Steer: focuses the composer, so it names the agent. */
  steerAgent: (name: string) => `Steer ${name}`,
  /** Agent-scoped Stop: stops the agent, not one run — the name says so. */
  stopAgent: (name: string) => `Stop ${name}`,
  steerRun: "Steer run",
  stopRun: "Stop run",
  sendSteer: "Send steer",
  /** Compact steer input: a name Activity's own input cannot collide with. */
  steerThisRunLabel: "Steer this run",
  /** Full Activity steer input. */
  steerSelectedRunLabel: "Steer selected run",
  stopSelectedRun: "Stop selected run",
  stop: "Stop",
  /** Thread-wide stop: names its scope so it cannot be read as one run. */
  stopAllRuns: "Stop all runs",
  /** Neutral trace left where the strip was after the operator stopped a run.
      Same words the Activity transcript uses for a cancelled turn. */
  runStopped: "Run stopped",
  seemsStuck: "seems stuck",
  workingFallback: "Working",
  retrying: (attempt: number, maxAttempts: number) =>
    `Retrying ${attempt}/${maxAttempts}`,
} as const;

/** Hide the live activity line until the turn has been alive this long. */
export const ACTIVITY_SILENCE_MS = 3_000;

/** Treat a turn as stuck when no new frame arrives for this long. */
export const ACTIVITY_STUCK_MS = 90_000;
