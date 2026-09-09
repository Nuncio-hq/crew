import { useAgentDirectory } from "./AgentDirectory";
import { ChannelRoleEditor, useChannelRoles } from "./ChannelRoles";
import { ThreadPeople } from "./ThreadPeople";
import { ThreadActivity } from "./ThreadActivity";
import { ThreadBoard, ThreadDetail } from "./ThreadViews";
import { useState, useRef, useEffect } from "react";
import { Icon, IconButton, Avatar, Status } from "./ui";
export function Conversation({
  initialActivity = false,
  filmFrame,
  threads,
  selectedThread,
  openThread,
  updateThread,
  screen,
  project,
  channel,
  status,
  setStatus,
  go,
  agent,
  tools,
  setTools,
  messages,
  send,
  draft,
  setDraft,
  scrollPositions,
  notice,
}) {
  const { displayName } = useAgentDirectory();
  const agentName = displayName(agent);
  const { contactPoint } = useChannelRoles(project, channel);
  const [activity, setActivity] = useState(initialActivity);
  const [choice, setChoice] = useState("");
  const [expanded, setExpanded] = useState(false);
  const scroll = useRef(null);
  const thread = screen === "thread",
    dm = screen === "dm",
    empty = screen === "empty";
  const key = dm
    ? `dm:${agent}`
    : thread
      ? `thread:${selectedThread.id}`
      : `${screen}:${project}:${channel}`;
  useEffect(() => {
    setActivity(initialActivity);
    setChoice("");
    if (scroll.current)
      scroll.current.scrollTop = scrollPositions.current[key] || 0;
  }, [key]);
  useEffect(() => {
    if (thread && (status === "needs-you" || status === "done")) {
      const node = scroll.current?.querySelector(
        status === "needs-you" ? ".question-card" : ".completion-card",
      );
      node?.scrollIntoView({ block: "nearest" });
    }
  }, [status, thread]);
  useEffect(() => {
    if (filmFrame && thread && scroll.current)
      scroll.current.scrollTop = scroll.current.scrollHeight;
  }, [filmFrame?.time, thread]);
  const heading = dm
    ? agentName
    : thread
      ? selectedThread.title
      : `# ${channel}`;
  return (
    <section
      className="conversation"
      aria-label={
        thread
          ? "Project thread"
          : dm
            ? `Direct message with ${agentName}`
            : "Project channel"
      }
    >
      <header className="conversation-header">
        <div className="title-line">
          {thread ? (
            <IconButton
              icon="back"
              label="Back to channel"
              onClick={() => go("channel")}
            />
          ) : (
            <Icon name={dm ? "message" : "hash"} />
          )}
          <span className="breadcrumb">{dm ? "Direct messages" : project}</span>
          <Icon name="right" size={12} />
          <span className="header-title">
            {dm ? agentName : thread ? `# ${channel}` : "Channel"}
          </span>
          <div className="spacer" />
          {thread && (
            <IconButton
              data-film="open-tools"
              icon="panel"
              label={tools ? "Hide thread tools" : "Open thread tools"}
              onClick={() => setTools(!tools)}
            />
          )}
        </div>
        <div className="conversation-heading">
          <h1>{heading}</h1>
          {thread ? (
            <Status state={status} />
          ) : (
            <span className="muted">
              {dm
                ? "Private conversation"
                : empty
                  ? "A new place to start"
                  : channel === "marketing"
                    ? "Campaigns, content and audience decisions"
                    : channel === "customers"
                      ? "Feedback and customer follow-ups"
                      : "A shared space for product decisions"}
            </span>
          )}
        </div>
        {empty && (
          <div className="channel-role-strip">
            <ThreadPeople project={project} channel={channel} />
            <ChannelRoleEditor project={project} channel={channel} />
          </div>
        )}
        {thread && (
          <div className="conversation-tabs">
            <button
              className={!activity ? "active" : ""}
              onClick={() => setActivity(false)}
            >
              Conversation
            </button>
            <button
              className={activity ? "active" : ""}
              onClick={() => setActivity(true)}
            >
              Activity{" "}
              <span className="count">
                {selectedThread.activity?.length || 0}
              </span>
            </button>
            <div className="spacer" />
            <span className="owner">
              <Avatar name={selectedThread.owner} small />
              {displayName(selectedThread.owner)} owns this
            </span>
          </div>
        )}
      </header>
      {status === "offline" && thread && (
        <div className="connection-banner" role="status">
          <Icon name="offline" />
          <span>Connection lost. Showing the last received updates.</span>
          <button
            onClick={() => {
              setStatus("working");
              notice("Connection restored — simulation");
            }}
          >
            Reconnect
          </button>
        </div>
      )}
      <div
        ref={scroll}
        className="message-scroll"
        onScroll={(e) => {
          scrollPositions.current[key] = e.currentTarget.scrollTop;
        }}
      >
        {activity && thread ? (
          <div className="activity-feed">
            <span className="eyebrow">
              {selectedThread.title} / SAMPLE ACTIVITY
            </span>
            <ThreadActivity thread={selectedThread} />
            <p className="muted">
              Activity records what happened. A finished turn is not an accepted
              outcome.
            </p>
          </div>
        ) : (
          <div className="message-column">
            {thread && !filmFrame && (
              <ThreadDetail
                thread={selectedThread}
                update={updateThread}
                hideSeed={
                  ["launch", "tracking"].includes(selectedThread.id) &&
                  !!messages[key]?.length
                }
              />
            )}
            {screen === "channel" && (
              <ThreadBoard
                threads={threads.filter(
                  (t) =>
                    (t.project || "NuncioCrew") === project &&
                    (t.channel || "product") === channel,
                )}
                openThread={openThread}
                channel={channel}
                project={project}
                prData={filmFrame?.prs}
              />
            )}
            {dm && (
              <>
                <div className="dm-intro">
                  <Avatar name={agent} />
                  <h2>{agentName}</h2>
                  <p>
                    This is the beginning of your direct conversation with{" "}
                    {agentName}.
                  </p>
                  <span className="muted">
                    Only the two of you in this conversation.
                  </span>
                </div>
                <div className="agent-message">
                  <div className="message-author">
                    <Avatar name={agent} />
                    <strong>{agentName}</strong>
                    <span>Today</span>
                  </div>
                  <div className="message-body">
                    <p>
                      What would you like to work on? We can explore an idea
                      here, then bring it into a project when you’re ready.
                    </p>
                  </div>
                </div>
              </>
            )}
            {empty && !messages[key]?.length && (
              <div className="empty-conversation">
                <Icon name="message" size={30} />
                <h2>Start the conversation</h2>
                <p>
                  Share an idea, ask a question, or bring an agent into the
                  work.
                </p>
              </div>
            )}
            {(messages[key] || []).map((m, i) => (
              <div
                key={i}
                className={
                  m.author === "Oscar" ? "user-message" : "agent-message"
                }
              >
                {m.author !== "Oscar" && (
                  <div className="message-author">
                    <Avatar name={m.author} />
                    <strong>{displayName(m.author)}</strong>
                    <span>{m.role || ""}</span>
                    <span>{m.time || "Just now"}</span>
                  </div>
                )}
                <p>
                  {m.text}
                  {m.streaming && (
                    <span className="stream-caret" aria-label="Writing" />
                  )}
                </p>
                {m.author === "Oscar" && (
                  <span>Oscar · {m.time || "Just now"}</span>
                )}
              </div>
            ))}
            {filmFrame?.typing && (
              <div className="typing-indicator">
                <Avatar name={filmFrame.typing} small />
                <span>{filmFrame.typing} is writing</span>
                <span className="typing-dots">
                  <i />
                  <i />
                  <i />
                </span>
              </div>
            )}
            {filmFrame && thread && status === "completed" && (
              <div className="completion-card">
                <Icon name="done" />
                <div>
                  <strong>Outcome accepted</strong>
                  <p>
                    {selectedThread.receipt?.replace(
                      " in this simulated story",
                      "",
                    )}
                  </p>
                </div>
              </div>
            )}
          </div>
        )}
      </div>
      <div className="composer-wrap">
        {thread && status === "working" && (
          <div className="working-line">
            <span className="tiny-dot green" />
            <span>{displayName(selectedThread.owner)} is working</span>
            <div className="spacer" />
            <button
              onClick={() => {
                setDraft("Please focus on ");
                document.getElementById("message-draft")?.focus();
              }}
            >
              Steer
            </button>
            <button onClick={() => setStatus("stopped")}>
              <Icon name="stop" size={12} />
              Stop
            </button>
          </div>
        )}
        <form
          className="composer"
          onSubmit={(e) => {
            e.preventDefault();
            if (draft.trim() && status !== "offline") {
              send(draft);
              setDraft("");
              setTimeout(
                () =>
                  scroll.current?.scrollTo({
                    top: scroll.current.scrollHeight,
                    behavior: "smooth",
                  }),
                30,
              );
            }
          }}
        >
          <textarea
            id="message-draft"
            data-film="composer"
            aria-label={
              dm
                ? `Message ${agentName}`
                : thread
                  ? "Reply in thread"
                  : "Message channel"
            }
            placeholder={
              dm
                ? `Message ${agentName}…`
                : thread
                  ? "Reply, ask a question, or steer the work…"
                  : `Message #${channel}…`
            }
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (
                e.key === "Enter" &&
                !e.shiftKey &&
                !e.nativeEvent.isComposing
              ) {
                e.preventDefault();
                e.currentTarget.form.requestSubmit();
              }
            }}
          />
          <div className="composer-bottom">
            <IconButton
              icon="plus"
              label="Attach context"
              onClick={() =>
                notice(
                  "Attachments are a future interaction; no file is uploaded.",
                )
              }
            />
            <span className="composer-context">
              {dm ? "Direct message" : "Project context"}
            </span>
            <div className="spacer" />
            <span className="composer-target">
              {dm
                ? agentName
                : contactPoint
                  ? `Contact · ${displayName(contactPoint)}`
                  : "@mention to get a reply"}
            </span>
            <button
              type="submit"
              data-film="send"
              className="send-button"
              aria-label="Send sample message"
              disabled={!draft.trim() || status === "offline"}
            >
              <Icon name="send" size={16} />
            </button>
          </div>
        </form>
        <span className="composer-hint">
          {status === "offline"
            ? "Draft kept while disconnected"
            : "Enter to send · Shift + Enter for a new line"}
        </span>
      </div>
    </section>
  );
}
