import type { ActiveTurnSummary } from "@/features/agents/activeAgentTurnsStore";

/** Channel compatibility policy; exact thread runs may narrow it at the caller. */
export function isAgentTurnLive(
  activeTurns: ActiveTurnSummary[],
  channelId: string | null,
  threadRuns?: readonly { sessionId: string }[],
) {
  if (threadRuns !== undefined) return threadRuns.length > 0;
  if (activeTurns.length === 0) return false;
  if (!channelId) return true;
  return activeTurns.some((turn) => turn.channelId === channelId);
}

/** Concurrent live sessions have no single current generation to label. */
export function currentThreadTranscriptSession(
  runs: readonly { sessionId: string }[],
): string | null {
  const sessions = new Set(runs.map((run) => run.sessionId));
  return sessions.size === 1 ? (sessions.values().next().value ?? null) : null;
}
