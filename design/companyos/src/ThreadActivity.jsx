import { useAgentDirectory } from "./AgentDirectory";
import { Avatar, Icon } from "./ui";
export function ThreadActivity({ thread, compact = false, onOpen }) {
  const { displayName, find } = useAgentDirectory();
  const rows = thread.activity || [];
  const recent = compact ? rows.slice(-2) : rows;
  const stale = thread.status === "offline";
  const active = thread.status === "working";
  return (
    <section
      className={`thread-stream ${compact ? "compact" : ""}`}
      aria-label={`Activity for ${thread.title}`}
    >
      <div className="stream-heading">
        <span>
          {stale ? "Disconnected · last received" : "Latest activity"}
        </span>
        {compact && (
          <button onClick={onOpen}>
            View activity <Icon name="right" size={12} />
          </button>
        )}
      </div>
      {!rows.length && (
        <p className="muted">No activity received for this thread.</p>
      )}
      {recent.map((row) => (
        <div className={`stream-event event-${row.kind}`} key={row.id}>
          <Avatar name={row.agent} small />
          <div>
            <div className="stream-byline">
              <strong>{displayName(row.agent)}</strong>
              <span>
                {row.kind === "thought"
                  ? "Thinking"
                  : row.kind === "message"
                    ? "Message"
                    : "Action"}
              </span>
              <span>
                {find(row.agent)?.deleted
                  ? "Removed · last received"
                  : row.status === "running"
                    ? stale
                      ? "Last known"
                      : active
                        ? "Running"
                        : "Interrupted"
                    : row.status === "failed"
                      ? "Failed"
                      : "Received"}
              </span>
            </div>
            <p>
              {row.text}
              {row.streaming &&
                active &&
                !stale &&
                !find(row.agent)?.deleted && (
                  <span className="stream-caret" aria-hidden="true" />
                )}
            </p>
            {row.kind === "tool" && row.status !== "running" && (
              <small className={row.failed ? "stream-failed" : ""}>
                {row.result}
              </small>
            )}
          </div>
        </div>
      ))}
      {stale && (
        <p className="muted">
          Current progress unknown. Reconnect to receive new activity.
        </p>
      )}
    </section>
  );
}
