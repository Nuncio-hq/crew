import { useRef } from "react";
import { useAgentDirectory } from "./AgentDirectory";
import { threadTeam } from "./thread-model";
import { Avatar, Icon } from "./ui";
const fixtures = {
  workspace: {
    Hermes: {
      source: "ACP plan",
      entries: [
        "Agree scope and assign the work",
        "Review delivery with Oscar",
      ],
    },
    Codex: {
      source: "ACP plan",
      entries: [
        "Inspect sidebar navigation",
        "Implement channel and thread routes",
        "Verify keyboard and pointer flows",
      ],
    },
    Claude: {
      source: "Structured todo tool",
      entries: [
        "Review navigation diff",
        "Report findings and verification gaps",
      ],
    },
  },
  remote: {
    Hermes: {
      source: "ACP plan",
      entries: ["Open the remote preview", "Verify the current deployment"],
    },
  },
};
export function AgentPlans({ thread, time = 0 }) {
  const { displayName, find } = useAgentDirectory();
  const participants = threadTeam(thread);
  const latestTimes = useRef({});
  return (
    <section className="agent-plans" aria-label="Agent plans">
      <h2>Agent plans & tasks</h2>
      <p className="field-help">
        Latest plan reported by each agent · sample events. No summarizer is
        used.
      </p>
      {participants.length === 0 && (
        <p>No agent plans received for this thread.</p>
      )}
      {participants.map(({ name }) => {
        const sample = fixtures[thread.id]?.[name];
        const stale = thread.status === "offline";
        const removed = find(name)?.deleted;
        const planKey = `${thread.id}:${name}`;
        if (!removed) latestTimes.current[planKey] = time;
        const receivedTime = removed
          ? (latestTimes.current[planKey] ?? 0)
          : time;
        const progressed = !stale && receivedTime >= 12;
        const completed = sample ? (progressed ? 1 : 0) : 0;
        return (
          <article className="agent-plan-card" key={name}>
            <header>
              <Avatar name={name} small />
              <strong>{displayName(name)}</strong>
              <span className="muted">
                {removed
                  ? "Removed · history"
                  : stale
                    ? "Disconnected · last known"
                    : "Reported plan"}
              </span>
            </header>
            {!sample ? (
              <p>No plan received. This does not mean the agent has no work.</p>
            ) : (
              <>
                <p className="field-help">
                  {sample.source} · snapshot {progressed ? "2" : "1"} ·{" "}
                  {completed}/{sample.entries.length} completed
                </p>
                <ol>
                  {sample.entries.map((content, i) => {
                    const state =
                      i < completed
                        ? "Completed"
                        : i === completed
                          ? "In progress"
                          : "Pending";
                    return (
                      <li key={content}>
                        <Icon
                          name={
                            state === "Completed"
                              ? "check"
                              : state === "In progress"
                                ? "clock"
                                : "circle"
                          }
                          size={14}
                        />
                        <span>{content}</span>
                        <small>{state}</small>
                      </li>
                    );
                  })}
                </ol>
                {(stale || removed || thread.status === "stopped") && (
                  <p className="field-help">
                    Last received snapshot; current execution is not confirmed.
                  </p>
                )}
              </>
            )}
          </article>
        );
      })}
      <p className="field-help">
        Plan completion is agent-reported progress. Acceptance and delivery
        evidence remain separate.
      </p>
    </section>
  );
}
