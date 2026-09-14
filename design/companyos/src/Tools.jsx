import { AgentPlans } from "./AgentPlans";
import { ThreadRecap } from "./RecapSettings";
import { useAgentDirectory } from "./AgentDirectory";
import { ThreadWorkspace, PRState } from "./ThreadWorkspace";
import { CampaignArtifact } from "./FilmPlayer";
import { pullRequests, nextStep } from "./thread-model";
import { useState } from "react";
import { Icon, IconButton, Status } from "./ui";
export function Tools({
  tab,
  setTab,
  close,
  status,
  thread,
  initialPR,
  filmArtifact,
  filmPRs,
  go,
  planTime,
  messageCount,
}) {
  const { displayName } = useAgentDirectory();
  const [selectedPR, setSelectedPR] = useState(
    thread.prs.includes(initialPR) ? initialPR : thread.prs[0],
  );
  const [logs, setLogs] = useState(false);
  const prData = { ...pullRequests, ...filmPRs };
  const pr = prData[selectedPR];
  const [file, setFile] = useState("PRODUCT.md");
  const [files, setFiles] = useState(false);
  const [preview, setPreview] = useState(true);
  return (
    <aside className="tools" aria-label="Thread tools">
      <div className="tool-tabs">
        {[
          ["file", "Files"],
          ["browser", "Browser"],
          ["pr", `PRs (${thread.prs.length})`],
          ["workflows", "Actions"],
          ["terminal", "Terminal"],
          ["plan", "Context"],
          ["agent-plans", "Agent plans"],
        ].map(([id, name]) => (
          <button
            key={id}
            data-film={`tool-${id}`}
            aria-label={name}
            title={name}
            onClick={() => setTab(id)}
            className={tab === id ? "active" : ""}
            aria-pressed={tab === id}
          >
            <Icon
              name={id === "agent-plans" ? "plan" : id === "plan" ? "info" : id}
              size={14}
            />
            <span>{name}</span>
          </button>
        ))}
        <div className="spacer" />
        <IconButton icon="close" label="Close tools" onClick={close} />
      </div>
      <div className="tool-scope">Thread · {thread.title}</div>
      {tab === "file" && filmArtifact && (
        <CampaignArtifact stage={filmArtifact} />
      )}
      {tab === "file" && !filmArtifact && (
        <>
          <div className="tool-address">
            <Icon name="file" size={13} />
            <span>crew / docs / {file}</span>
            <div className="spacer" />
            <IconButton
              icon="sidebar"
              label="Toggle file list"
              onClick={() => setFiles(!files)}
            />
          </div>
          <div className="file-layout">
            <article className="document">
              <span className="eyebrow">NUNCIOCREW / PRODUCT</span>
              <h1>
                {file === "PRODUCT.md"
                  ? "A place to get work done."
                  : file === "ARCHITECTURE.md"
                    ? "The relay is the workspace."
                    : "Build. Verify. Hand back."}
              </h1>
              <p className="doc-lead">
                Your projects, people and conversations.
                <br />
                Together, with room to focus.
              </p>
              <hr />
              <h2>The workspace</h2>
              <p>
                Move between a project conversation and a focused thread without
                losing the context of the work.
              </p>
              <h3>Projects organize the work</h3>
              <p>
                A project brings related conversations into view. Open a channel
                for the bigger picture, or pick up a recent thread to continue a
                specific task.
              </p>
              <h3>Agents are people you work with</h3>
              <p>
                Talk directly to an agent, or bring specialists into a project
                thread. See who owns the next step and when they need your
                input.
              </p>
              <h3>Focus stays with the conversation</h3>
              <p>
                Keep the conversation readable. Open a document, a preview or a
                pull request beside it when the work calls for one.
              </p>
              <blockquote>
                The result matters. Activity helps you understand how we got
                there.
              </blockquote>
              <h2>A complete delivery loop</h2>
              <ol>
                <li>Agree on the outcome and scope.</li>
                <li>Delegate, implement and verify.</li>
                <li>Return evidence for review.</li>
              </ol>
              <div className="doc-footer">
                <Icon name="file" size={14} /> Product brief · example content
              </div>
            </article>
            {files && (
              <nav className="file-list" aria-label="Example files">
                <span>docs / crew</span>
                {["PRODUCT.md", "ARCHITECTURE.md", "TESTING.md"].map((n) => (
                  <button
                    key={n}
                    className={file === n ? "active" : ""}
                    onClick={() => setFile(n)}
                  >
                    <Icon name="file" size={13} />
                    {n}
                  </button>
                ))}
              </nav>
            )}
          </div>
        </>
      )}
      {tab === "browser" && (
        <>
          <div className="tool-address">
            <Icon name="browser" size={14} />
            <span>Workspace preview</span>
            <div className="spacer" />
            <IconButton
              icon="retry"
              label="Reload example preview"
              onClick={() => setPreview(true)}
            />
          </div>
          {preview ? (
            <div className="browser-example">
              <div className="example-nav">
                <strong>NuncioCrew</strong>
                <span>Workspace</span>
              </div>
              <div className="example-body">
                <span className="eyebrow">YOUR WORK, IN ONE PLACE</span>
                <h1>
                  A little less coordination.
                  <br />A lot more progress.
                </h1>
                <p>
                  Work with your team of agents, from the first conversation to
                  the finished result.
                </p>
                <button
                  className="light-button"
                  onClick={() => setPreview(false)}
                >
                  Explore workspace <Icon name="out" size={14} />
                </button>
              </div>
            </div>
          ) : (
            <div className="tool-empty">
              <Icon name="done" size={28} />
              <h2>Preview interaction complete</h2>
              <p>This example stays inside the prototype.</p>
              <button onClick={() => setPreview(true)}>
                Return to preview
              </button>
            </div>
          )}
          <div className="tool-footnote">
            Sample page · no dev server or external website
          </div>
        </>
      )}
      {(tab === "pr" || tab === "workflows") && (
        <article className="tool-content">
          <span className="eyebrow">
            {tab === "pr" ? "LINKED PULL REQUESTS" : "ACTIONS / CI & DELIVERY"}
          </span>
          <h1>
            {tab === "pr"
              ? `${thread.prs.length} linked pull requests`
              : "Checks & deployments"}
          </h1>
          <p className="muted">
            Sample evidence · updated just now. Each result belongs to a
            specific revision.
          </p>
          {thread.prs.length === 0 ? (
            <p>
              No linked pull requests or workflow runs for this thread. Non-code
              work can finish without CI.
            </p>
          ) : (
            <>
              <div className="pr-picker" aria-label="Choose pull request">
                {thread.prs.map((id) => (
                  <button
                    key={id}
                    aria-pressed={id === selectedPR}
                    onClick={() => {
                      setSelectedPR(id);
                      setLogs(false);
                    }}
                  >
                    <strong>
                      #{prData[id].number} {prData[id].title}
                    </strong>
                    <span>
                      {prData[id].state} · CI {prData[id].checks}
                    </span>
                  </button>
                ))}
              </div>
              {pr && (
                <>
                  <h2>
                    #{pr.number} {pr.title}
                  </h2>
                  <p>
                    {pr.repo} · <code>{pr.head}</code>
                  </p>
                  <dl className="evidence-grid">
                    <dt>Pull request</dt>
                    <dd>
                      <PRState pr={pr} />
                    </dd>
                    <dt>Review</dt>
                    <dd>{pr.review}</dd>
                    <dt>Required CI</dt>
                    <dd>{pr.checks}</dd>
                    <dt>Deployment</dt>
                    <dd>{pr.deploy}</dd>
                  </dl>
                  {tab === "pr" ? (
                    <div className="code-diff">
                      <div>
                        PR changes · {pr.branch || "branch unavailable"} →{" "}
                        {pr.base || "main"}
                      </div>
                      <div className="pr-diff-counts">
                        <span className="diff-add">+{pr.additions ?? "?"}</span>{" "}
                        <span className="diff-remove">
                          −{pr.deletions ?? "?"}
                        </span>{" "}
                        · PR diff at {pr.head}
                      </div>
                      <p>+ {pr.title}</p>
                      <p>Review and check results are scoped to {pr.head}.</p>
                    </div>
                  ) : (
                    <>
                      <div className="run-card">
                        <strong>Verify · run #{pr.number}2</strong>
                        <p>pull_request · {pr.head} · attempt 1</p>
                        <div className="plan-item">
                          <Icon name="check" />
                          <span>Format & types</span>
                          <small>Passed</small>
                        </div>
                        <div className="plan-item">
                          <Icon
                            name={pr.checks === "Passed" ? "check" : "clock"}
                          />
                          <span>Integration tests</span>
                          <small>{pr.checks}</small>
                        </div>
                        <button
                          aria-expanded={logs}
                          onClick={() => setLogs(!logs)}
                        >
                          {logs ? "Hide" : "View"} job logs
                        </button>
                        {logs && (
                          <pre className="tool-output">
                            {pr.checks === "Failed"
                              ? "FAIL reconnect.spec\nExpected a fresh subscription after reconnect.\nNo completion evidence emitted."
                              : pr.checks === "Running"
                                ? "Integration tests in progress…\nFinal result not yet available."
                                : "Required checks passed for " +
                                  pr.head +
                                  "\nThis is sample output, not an executed check."}
                          </pre>
                        )}
                      </div>
                      <div className="run-card">
                        <strong>Delivery</strong>
                        <p>{pr.deploy}</p>
                        <small>
                          {thread.id === "release"
                            ? "Staging · release-d2 · health check passed. Production has no run yet."
                            : "No deployment run required by this thread’s current scope."}
                        </small>
                      </div>
                    </>
                  )}
                </>
              )}
            </>
          )}
          <p className="muted">
            Rerun, cancel, merge and deploy controls require a separate
            permissions design. This preview makes no GitHub requests.
          </p>
        </article>
      )}
      {tab === "terminal" && (
        <div className="terminal-content">
          <div className="terminal-label">Verification / sample output</div>
          <pre>
            {
              "$ npm run check\n\n✓ Sidebar navigation\n✓ Channel → thread → back\n✓ Conversation width guard\n✓ Draft survives reconnect\n\n4 checks illustrated, not executed\n\n$ _"
            }
          </pre>
        </div>
      )}
      {tab === "agent-plans" && (
        <article className="tool-content">
          <AgentPlans thread={thread} time={planTime} />
        </article>
      )}
      {tab === "plan" && (
        <article className="tool-content">
          <span className="eyebrow">THREAD CONTEXT</span>
          <h1>{thread.title}</h1>
          <Status state={status} />
          <ThreadRecap thread={thread} go={go} messageCount={messageCount} />
          <button
            className="view-agent-plans"
            onClick={() => setTab("agent-plans")}
          >
            View agent plans & tasks
          </button>
          <h2>Agreed outcome</h2>
          <p>{thread.criteria}</p>
          <h2>Next step</h2>
          <p>
            {thread.status === "working"
              ? "See Activity for the latest received execution events."
              : nextStep(thread)}
          </p>
          <h2>Owner</h2>
          <p>{displayName(thread.owner)}</p>
          <h2>Completion evidence</h2>
          <p>
            {thread.receipt ||
              "No accepted completion receipt yet. A finished turn or green CI alone does not close the work."}
          </p>
          <h2>Workspace</h2>
          {thread.workspace ? (
            <>
              <ThreadWorkspace thread={thread} />
              <code className="workspace-full-path">
                {thread.workspace.path}
              </code>
            </>
          ) : (
            <p>No worktree attached to this thread.</p>
          )}
          <p className="muted">
            Selecting this thread does not create a new worktree.
          </p>
        </article>
      )}
    </aside>
  );
}
