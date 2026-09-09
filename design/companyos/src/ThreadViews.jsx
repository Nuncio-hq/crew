import { ThreadWorkspace, prStatus, prIcon } from "./ThreadWorkspace";
import { useAgentDirectory } from "./AgentDirectory";
import { ChannelRoleEditor } from "./ChannelRoles";
import { ThreadActivity } from "./ThreadActivity";
import { ThreadPeople } from "./ThreadPeople";
import { useState } from "react";
import { Status, Icon } from "./ui";
import { needsAttention, nextStep, pullRequests } from "./thread-model";
export function ThreadBoard({
  threads,
  openThread,
  channel = "product",
  project = "NuncioCrew",
  prData = pullRequests,
}) {
  const { displayName } = useAgentDirectory();
  const [filter, setFilter] = useState("All");
  const choices = ["All", "Needs you", "In progress", "Done", "Discussion"];
  const match = (t) =>
    filter === "All" ||
    (filter === "Needs you"
      ? needsAttention(t)
      : filter === "In progress"
        ? ["working", "blocked", "stopped", "offline"].includes(t.status)
        : filter === "Done"
          ? t.status === "completed"
          : t.status === "discussion");
  return (
    <div className="thread-board">
      <div className="channel-intro">
        <span className="eyebrow">CHANNEL OVERVIEW</span>
        <h2>
          {channel === "marketing"
            ? "Marketing conversations"
            : channel === "engineering"
              ? "Engineering conversations"
              : channel === "customers"
                ? "Customer conversations"
                : "Product conversations"}
        </h2>
        <p>
          {threads.filter(needsAttention).length} need you ·{" "}
          {
            threads.filter((t) =>
              ["working", "blocked", "stopped", "offline"].includes(t.status),
            ).length
          }{" "}
          in progress · {threads.filter((t) => t.status === "completed").length}{" "}
          done
        </p>
      </div>
      <div className="channel-role-strip">
        <ThreadPeople project={project} channel={channel} />
        <ChannelRoleEditor project={project} channel={channel} />
      </div>
      <div className="thread-filters" aria-label="Filter channel threads">
        {choices.map((f) => (
          <button
            key={f}
            aria-pressed={filter === f}
            onClick={() => setFilter(f)}
          >
            {f}
          </button>
        ))}
      </div>
      {threads.filter(match).map((t, i) => (
        <article
          data-thread={t.id}
          data-phase={t.demoPhase}
          style={{ "--card-delay": `${Math.min(i * 25, 100)}ms` }}
          className={`thread-summary state-${t.status}`}
          key={t.id}
        >
          <div className="thread-meta">
            <Status state={t.status} />
            {t.demoPhase && t.status === "working" && (
              <span className="phase-chip">{t.demoPhase}</span>
            )}
            <span className="lead-label">
              {displayName(t.owner)} · Thread owner
            </span>
            {needsAttention(t) && (
              <span className="attention-label">Your action</span>
            )}
          </div>
          <button
            data-film={`thread-${t.id}`}
            className="thread-title"
            onClick={() => openThread(t.id)}
          >
            {t.title}
            <Icon name="right" />
          </button>
          <p>{t.summary}</p>
          <ThreadWorkspace thread={t} onOpen={() => openThread(t.id, "plan")} />
          {needsAttention(t) ||
          ["completed", "discussion", "stopped"].includes(t.status) ? (
            <p className="thread-next" key={t.status + t.next}>
              <Icon
                name={
                  t.status === "blocked"
                    ? "alert"
                    : needsAttention(t)
                      ? "info"
                      : t.status === "completed"
                        ? "check"
                        : "right"
                }
                size={14}
              />
              <span>{nextStep(t)}</span>
            </p>
          ) : (
            <ThreadActivity
              thread={t}
              compact
              onOpen={() => openThread(t.id, "activity")}
            />
          )}
          <ThreadPeople thread={t} />
          <div className="thread-card-footer">
            <button className="reply-count" onClick={() => openThread(t.id)}>
              <Icon name="message" size={13} />
              {t.replies} replies
            </button>
            {t.status === "done" && (
              <button
                className="review-result"
                onClick={() => openThread(t.id)}
              >
                Review result
                <Icon name="right" size={12} />
              </button>
            )}
            <div className="pr-badges">
              {t.prs.length ? (
                t.prs.map((id) => {
                  const pr = prData[id] || pullRequests[id];
                  return (
                    <div className="pr-pair" key={id}>
                      <button
                        className={`pr-badge pr-${prStatus(pr)}`}
                        aria-label={`Open PR ${pr.number}: ${pr.title}, ${pr.state}, CI ${pr.checks}`}
                        onClick={() => openThread(t.id, "pr", id)}
                      >
                        <Icon name={prIcon(pr)} size={13} />
                        <span>#{pr.number}</span>
                        <span>{pr.state}</span>
                      </button>
                      <span
                        className={`ci-chip check-${pr.checks.toLowerCase()}`}
                      >
                        <Icon
                          name={
                            pr.checks === "Passed"
                              ? "check"
                              : pr.checks === "Running"
                                ? "spinner"
                                : "close"
                          }
                          size={12}
                        />
                        <span>CI {pr.checks}</span>
                      </span>
                    </div>
                  );
                })
              ) : (
                <span className="no-pr">No linked PRs</span>
              )}
            </div>
          </div>
        </article>
      ))}
    </div>
  );
}
export function ThreadDetail({ thread: t, update, hideSeed = false }) {
  const [choice, setChoice] = useState("");
  return (
    <div className="thread-detail">
      <ThreadPeople thread={t} expandedByDefault />
      {!hideSeed && (
        <>
          <div className="date-divider">Today · sample conversation</div>
          <div className="user-message">
            <p>{t.summary}</p>
            <span>Oscar</span>
          </div>
          <div className="agent-message">
            <strong>{t.owner}</strong>
            <p>
              {t.status === "working"
                ? "I’ll coordinate the agreed work here and bring back the result for review."
                : nextStep(t)}
            </p>
          </div>
        </>
      )}
      {t.receipt && t.status !== "completed" && (
        <p className="muted">Previous acceptance: {t.receipt}</p>
      )}
      <div className="completion-criteria">
        <span className="eyebrow">AGREED OUTCOME</span>
        <p>{t.criteria}</p>
      </div>
      {t.status === "needs-you" && (
        <section className="question-card">
          <h3>Which release scope should I use?</h3>
          {["Staging only", "Include production after review"].map((c) => (
            <label key={c}>
              <input
                type="radio"
                name="scope"
                checked={choice === c}
                onChange={() => setChoice(c)}
              />
              {c}
            </label>
          ))}
          <button
            className="light-button"
            disabled={!choice}
            onClick={() =>
              update({
                status: "working",
                next: `Oscar chose: ${choice}. Owner is preparing the next step.`,
              })
            }
          >
            Confirm choice
          </button>
          <p className="muted">
            This records a sample decision; it does not deploy.
          </p>
        </section>
      )}
      {t.status === "done" && (
        <section className="completion-card">
          <Icon name="check" />
          <div>
            <strong>Ready for your review</strong>
            <p>
              {t.id === "handoff"
                ? "Delivered: delivery checklist v2. Walkthrough covers scope, evidence, owner and next action."
                : "Review the deliverables and evidence in this conversation against the agreed outcome."}
            </p>
            <p>Limits: no production runtime was exercised.</p>
            <div className="thread-filters">
              <button
                onClick={() =>
                  update({
                    status: "completed",
                    receipt: `Oscar accepted ${t.title} · just now (simulation).`,
                  })
                }
              >
                Accept result
              </button>
              <button
                onClick={() =>
                  update({
                    status: "working",
                    next: "Oscar requested changes to the delivery checklist.",
                  })
                }
              >
                Request changes
              </button>
            </div>
          </div>
        </section>
      )}
      {t.status === "completed" && (
        <section className="completion-card">
          <Icon name="done" />
          <div>
            <strong>Done · accepted</strong>
            <p>{t.receipt}</p>
            <button
              onClick={() =>
                update({
                  status: "working",
                  next: "Oscar reopened this work · follow-up required.",
                })
              }
            >
              Reopen work
            </button>
          </div>
        </section>
      )}
      {t.status === "blocked" && (
        <section className="blocked-card">
          <strong>CI failed · owner is handling it</strong>
          <p>
            Reconnect test failed at retry-f4. Open Actions for the failed job
            and logs. This does not require your input yet.
          </p>
        </section>
      )}
      {t.status === "offline" && (
        <section className="connection-banner">
          <p>
            Last-known state only. Reconnect must fetch current evidence;
            silence does not mean completion.
          </p>
        </section>
      )}
    </div>
  );
}
