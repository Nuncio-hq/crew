import { useEffect, useRef, useState } from "react";
import { Icon } from "./ui";
import { useProjects } from "./ProjectModel";
import { useAgentDirectory } from "./AgentDirectory";
import { useWiki, wikiKey, generationSteps } from "./WikiModel";
import { wikiPages } from "./wiki-content";
import { WikiArticle, WikiSourcePanel, WikiToc } from "./WikiReading";
import { WikiAnswer, WikiComposer } from "./WikiAsk";
import {
  WikiGeneratorDialog,
  WikiHistoryDialog,
  WikiSearchDialog,
  WikiTaskDialog,
} from "./WikiDialogs";
import { ProjectDialog } from "./ProjectDialogs";
import "./wiki.css";

export function WikiPage({ projectName, scenario, go, notice, onStartThread }) {
  const { projects } = useProjects(),
    { agents } = useAgentDirectory(),
    wiki = useWiki();
  const project = projects.find((p) => p.name === projectName) || projects[0];
  const selected = wiki.repositories[project.name];
  const workspace =
    project.workspaces.find((w) => w.id === selected) || project.workspaces[0];
  const key = wikiKey(project.name, workspace?.id),
    record = wiki.get(key);
  const page =
    record.pages.find((p) => p.id === record.page) || record.pages[0];
  const question = record.history.find((q) => q.id === record.activeQuestion);
  const projectAgents = agents.filter((a) =>
    a.channels.some((c) => c.startsWith(`${project.name}/`)),
  );
  const [dialog, setDialog] = useState(null),
    sourceTrigger = useRef(null);
  const connected = workspace?.available && record.health !== "missing";
  const patch = (change) => wiki.patch(key, change);
  useEffect(() => {
    if (!scenario.startsWith("wiki-")) return;
    const state = scenario.slice(5);
    patch((r) => ({
      ...r,
      source: null,
      view: "read",
      health:
        state === "unavailable" || state === "answer-failed" ? "ready" : state,
      pages: state === "empty" ? [] : wikiPages,
      page: "workspaces",
      job: null,
      askUnavailable: state === "unavailable",
      askFail: state === "answer-failed",
    }));
    if (state === "updating")
      wiki.generate(
        key,
        { ...workspace, project: project.name },
        record.settings.runtime
          ? record.settings
          : { runtime: "Codex", model: "", profile: "" },
      );
  }, [scenario, key]);
  function choosePage(id) {
    patch({ page: id, view: "read", source: null });
    setDialog(null);
  }
  function openSource(id, trigger) {
    sourceTrigger.current = trigger;
    patch({ source: id });
  }
  function closeSource() {
    patch({ source: null });
    requestAnimationFrame(
      () => sourceTrigger.current?.isConnected && sourceTrigger.current.focus(),
    );
  }
  function ask(prompt) {
    const a = projectAgents.find((a) => a.id === record.askAgent);
    if (record.askUnavailable || !a || a.status !== "Available") {
      notice("Choose an available project agent before asking.");
      return;
    }
    if (question?.state === "reading") {
      notice("Wait for this answer or stop it first.");
      return;
    }
    wiki.ask(key, prompt, a.id);
  }
  const busy = question?.state === "reading";
  return (
    <section className="wiki-page" aria-label={`${project.name} Wiki`}>
      <header className="wiki-header">
        <div className="wiki-breadcrumb">
          <Icon name="wiki" size={17} />
          <button
            className="project-crumb"
            onClick={() => go("project", { project: project.name })}
          >
            {project.name}
          </button>
          <Icon name="right" size={12} />
          <strong>Wiki</strong>
          {project.workspaces.length > 1 ? (
            <select
              aria-label="Wiki repository"
              value={workspace.id}
              onChange={(e) =>
                wiki.selectRepository(project.name, e.target.value)
              }
            >
              {project.workspaces.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name}
                </option>
              ))}
            </select>
          ) : (
            workspace && (
              <span className="wiki-repository-name">{workspace.name}</span>
            )
          )}
        </div>
        <div className="wiki-header-actions">
          <button
            aria-label="Search Wiki"
            disabled={!record.pages.length}
            onClick={() => setDialog("search")}
          >
            <Icon name="search" size={15} />
            <span>Search</span>
          </button>
          <button
            aria-label="Wiki question history"
            onClick={() => setDialog("history")}
          >
            <Icon name="clock" size={15} />
            <span>History</span>
          </button>
          <button
            className="wiki-update-button"
            disabled={!connected || record.health === "updating"}
            onClick={() => setDialog("generate")}
          >
            <Icon name="retry" size={14} />
            <span>{record.pages.length ? "Update Wiki" : "Generate Wiki"}</span>
          </button>
        </div>
      </header>
      {!!record.pages.length && (
        <div className="wiki-subheader">
          <div className="wiki-view-tabs" role="group" aria-label="Wiki view">
            <button
              aria-pressed={record.view === "read"}
              onClick={() => patch({ view: "read", source: null })}
            >
              Read
            </button>
            <button
              aria-pressed={record.view === "ask"}
              onClick={() => patch({ view: "ask", source: null })}
            >
              Ask
            </button>
          </div>
          <button
            className="wiki-contents-toggle"
            onClick={() => setDialog("contents")}
          >
            <Icon name="plan" size={14} />
            Contents
          </button>
          <span className="spacer" />
          <span className="wiki-snapshot">
            <Icon name="branch" size={12} />
            {workspace?.branch || "Snapshot"}
            <code>{record.revision.slice(0, 7)}</code>
            <span>Updated {record.updated.toLowerCase()}</span>
          </span>
        </div>
      )}
      {record.health === "updating" && (
        <div className="wiki-status-banner" role="status">
          <span className="wiki-spinner" />
          <span>
            <strong>Updating Wiki</strong> ·{" "}
            {generationSteps[record.job?.step || 0]}
          </span>
          <span className="spacer" />
          <button onClick={() => wiki.cancelUpdate(key)}>Cancel update</button>
        </div>
      )}
      {record.health === "failed" && (
        <div className="wiki-status-banner warning" role="status">
          <Icon name="alert" size={16} />
          <span>
            <strong>Update failed.</strong> The last snapshot is still
            available.
          </span>
          <span className="spacer" />
          <button disabled={!connected} onClick={() => setDialog("generate")}>
            Retry update
          </button>
        </div>
      )}
      {record.health === "stale" && (
        <div className="wiki-status-banner warning">
          <Icon name="clock" size={15} />
          <span>Source changed since this snapshot.</span>
          <span className="spacer" />
          <button disabled={!connected} onClick={() => setDialog("generate")}>
            Review update
          </button>
        </div>
      )}
      {!connected && !!record.pages.length && (
        <div className="wiki-status-banner warning" role="status">
          <Icon name="folder" size={15} />
          <span>
            Workspace unavailable. You can still read and ask about the saved
            snapshot.
          </span>
          <button
            onClick={() =>
              record.health === "missing"
                ? patch({ health: "ready" })
                : go("project", { project: project.name })
            }
          >
            {record.health === "missing"
              ? "Retry connection"
              : "Manage workspace"}
          </button>
        </div>
      )}
      {record.pages.length ? (
        <div
          className={`wiki-content-layout ${record.source ? "source-open" : ""} ${record.view === "ask" ? "asking" : ""}`}
        >
          {!record.source && record.view === "read" && (
            <WikiToc
              pages={record.pages}
              selected={page.id}
              onSelect={choosePage}
            />
          )}
          <div className="wiki-main-column">
            {record.view === "read" ? (
              <WikiArticle
                key={page.id}
                page={page}
                record={record}
                onSource={openSource}
                onSelect={choosePage}
                onScroll={(id, top) =>
                  patch((r) => ({ ...r, scroll: { ...r.scroll, [id]: top } }))
                }
              />
            ) : (
              <WikiAnswer
                key={question?.id || "new"}
                question={question}
                onSource={openSource}
                onAsk={ask}
                onDraft={() => setDialog("task")}
                hasDraft={!!record.taskDraft}
                onRead={choosePage}
                onStop={() => wiki.cancelAsk(key)}
              />
            )}
            <WikiComposer
              record={record}
              page={page}
              agents={projectAgents}
              busy={busy}
              onAgent={(id) => patch({ askAgent: id, askUnavailable: false })}
              onDraft={(questionDraft) => patch({ questionDraft })}
              onAsk={ask}
              onStop={() => wiki.cancelAsk(key)}
            />
          </div>
          {record.source && (
            <WikiSourcePanel
              id={record.source}
              onClose={closeSource}
              onNotice={notice}
            />
          )}
        </div>
      ) : (
        <div className="wiki-empty">
          <div className="wiki-empty-icon">
            <Icon name="wiki" size={30} />
          </div>
          <span className="eyebrow">{project.name} WIKI</span>
          <h1>
            {!workspace
              ? "Give your project a source."
              : record.health === "updating"
                ? "Your first Wiki is on its way."
                : "Turn this workspace into a Wiki."}
          </h1>
          <p>
            {!workspace
              ? "Link a repository or folder in Project settings, then generate its first pages."
              : !connected
                ? "This workspace is unavailable on this machine. Choose an accessible folder to generate pages."
                : "A readable guide to the project, with answers you can trace back to the source."}
          </p>
          {record.health !== "updating" && (
            <button
              className="project-button primary"
              onClick={() =>
                connected
                  ? setDialog("generate")
                  : go("project", { project: project.name })
              }
            >
              <Icon name={connected ? "sparkles" : "folder"} size={15} />
              {connected ? "Generate Wiki" : "Manage workspace"}
            </button>
          )}
          <small>Prototype · generation and questions are simulated</small>
        </div>
      )}
      {dialog === "generate" && workspace && (
        <WikiGeneratorDialog
          record={record}
          workspace={workspace}
          onClose={() => setDialog(null)}
          onGenerate={(settings) => {
            wiki.generate(
              key,
              { ...workspace, project: project.name },
              settings,
            );
            setDialog(null);
          }}
        />
      )}
      {dialog === "search" && (
        <WikiSearchDialog
          pages={record.pages}
          onSelect={choosePage}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog === "history" && (
        <WikiHistoryDialog
          history={record.history}
          onSelect={(id) => {
            patch({ activeQuestion: id, view: "ask", source: null });
            setDialog(null);
          }}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog === "contents" && (
        <ProjectDialog
          title="Wiki contents"
          subtitle={project.name}
          action="Done"
          onClose={() => setDialog(null)}
          onSubmit={(e) => {
            e.preventDefault();
            setDialog(null);
          }}
        >
          <WikiToc
            pages={record.pages}
            selected={page.id}
            onSelect={choosePage}
          />
        </ProjectDialog>
      )}
      {dialog === "task" && question && (
        <WikiTaskDialog
          record={record}
          question={question}
          project={project}
          agents={agents}
          onClose={() => setDialog(null)}
          onSave={(draft) => {
            patch({ taskDraft: draft });
            setDialog(null);
            notice("Private task draft saved in this preview");
          }}
          onStart={(draft) => {
            patch({ taskDraft: null });
            setDialog(null);
            onStartThread(draft, {
              project: project.name,
              repository: workspace?.id,
              page: page.id,
              question: draft.questionId,
              revision: record.revision,
            });
          }}
        />
      )}
    </section>
  );
}
