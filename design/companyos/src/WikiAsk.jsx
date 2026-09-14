import { Icon, Avatar } from "./ui";
import { SourceCitations } from "./WikiReading";

export function WikiComposer({
  record,
  page,
  agents,
  onAgent,
  onDraft,
  onAsk,
  onStop,
  busy,
}) {
  const selected = agents.find((a) => a.id === record.askAgent);
  const unavailable =
    record.askUnavailable || !selected || selected.status !== "Available";
  const submit = () => {
    if (!unavailable && !busy && record.questionDraft.trim())
      onAsk(record.questionDraft.trim());
  };
  return (
    <div className="wiki-composer-wrap">
      <form
        className="wiki-composer"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <label className="sr-only" htmlFor="wiki-question">
          Ask about this repository
        </label>
        <textarea
          id="wiki-question"
          rows={2}
          maxLength={4000}
          placeholder={
            record.view === "ask"
              ? "Ask a follow-up question…"
              : "Ask about this repository…"
          }
          value={record.questionDraft}
          onChange={(e) => onDraft(e.target.value)}
          onKeyDown={(e) => {
            if (
              e.key === "Enter" &&
              !e.shiftKey &&
              !e.nativeEvent.isComposing
            ) {
              e.preventDefault();
              submit();
            }
          }}
        />
        <div className="wiki-composer-bottom">
          <label className="wiki-agent-picker">
            <Icon name="agents" size={14} />
            <span className="sr-only">Ask agent</span>
            <select
              aria-label="Ask agent"
              value={record.askAgent}
              onChange={(e) => onAgent(e.target.value)}
            >
              {!selected && (
                <option value={record.askAgent}>
                  {record.askAgent} · unavailable
                </option>
              )}
              {agents.map((a) => (
                <option
                  key={a.id}
                  value={a.id}
                  disabled={a.status !== "Available"}
                >
                  {a.name}
                  {a.status !== "Available"
                    ? ` · ${a.status.toLowerCase()}`
                    : ""}
                </option>
              ))}
            </select>
          </label>
          <span className="wiki-composer-scope">
            {page?.title || "Repository snapshot"}
          </span>
          {busy ? (
            <button
              className="wiki-send"
              type="button"
              aria-label="Stop answer"
              onClick={onStop}
            >
              <Icon name="stop" size={15} />
            </button>
          ) : (
            <button
              className="wiki-send"
              type="submit"
              aria-label="Send Wiki question"
              disabled={unavailable || !record.questionDraft.trim()}
            >
              <Icon name="send" size={16} />
            </button>
          )}
        </div>
        {unavailable && (
          <p className="wiki-composer-error" role="status">
            {selected?.name || record.askAgent} is unavailable. Choose another
            available project agent.
          </p>
        )}
      </form>
      <div className="wiki-private-note">
        Private questions · sample answers · Enter to send, Shift + Enter for a
        new line
      </div>
    </div>
  );
}

export function WikiAnswer({
  question,
  onSource,
  onAsk,
  onDraft,
  onRead,
  onStop,
  hasDraft,
}) {
  if (!question)
    return (
      <div className="wiki-answer-scroll">
        <div className="wiki-answer-intro">
          <Icon name="wiki" size={28} />
          <h1>Understand this repository.</h1>
          <p>Ask a question, check the code, then decide what to do next.</p>
          <div className="wiki-suggestions">
            {[
              "How do thread workspaces work?",
              "How is a plain folder different?",
              "How are Wiki pages updated?",
            ].map((q) => (
              <button key={q} onClick={() => onAsk(q)}>
                {q}
                <Icon name="right" size={14} />
              </button>
            ))}
          </div>
        </div>
      </div>
    );
  return (
    <div className="wiki-answer-scroll">
      <article className="wiki-answer">
        <div className="wiki-question-bubble">
          <span>You asked</span>
          <h1>{question.prompt}</h1>
        </div>
        <div className="wiki-answer-author">
          <Avatar name={question.agent} small />
          <strong>{question.agent}</strong>
          <span>Example answer · {question.revision.slice(0, 7)}</span>
        </div>
        {question.state === "reading" ? (
          <div className="wiki-answer-progress" role="status">
            <span className="wiki-spinner" />
            <div>
              <strong>Reading the Wiki and source references…</strong>
              <p>Scoped to this repository snapshot.</p>
            </div>
            <button onClick={onStop}>Stop</button>
          </div>
        ) : question.state === "failed" || question.state === "stopped" ? (
          <div className="wiki-answer-recovery" role="status">
            <h2>
              {question.state === "failed"
                ? "The answer could not finish"
                : "Answer stopped"}
            </h2>
            <p>
              Your question is saved. You can retry when the agent is available.
            </p>
            <button
              className="project-button"
              onClick={() => onAsk(question.prompt)}
            >
              Retry question
            </button>
          </div>
        ) : question.answer.missing ? (
          <div className="wiki-answer-recovery">
            <h2>No supported answer in this sample</h2>
            <p>
              This prototype has prepared answers about workspaces, folders,
              leases and Wiki updates. It cannot infer an answer outside that
              sample.
            </p>
            <button
              className="project-button"
              onClick={() => onAsk("How do thread workspaces work?")}
            >
              Ask about thread workspaces
            </button>
          </div>
        ) : (
          <>
            {question.answer.paragraphs.map((p) => (
              <section key={p.title}>
                <h2>{p.title}</h2>
                <p>{p.text}</p>
                <SourceCitations ids={p.sources} onSource={onSource} />
              </section>
            ))}
            <div className="wiki-answer-actions">
              <button className="project-button primary" onClick={onDraft}>
                <Icon name="plus" size={14} />
                {hasDraft ? "Resume task draft" : "Create task draft"}
              </button>
              <button
                className="project-button"
                onClick={() => onRead(question.answer.pageId)}
              >
                <Icon name="wiki" size={14} />
                Read related page
              </button>
            </div>
            <p className="wiki-answer-private">
              This answer is private. Create a draft to choose what goes into a
              channel.
            </p>
          </>
        )}
      </article>
    </div>
  );
}
