import { useState } from "react";
import { ProjectDialog } from "./ProjectDialogs";
import { sampleRuntimes } from "./RecapSettings";
import { sampleProfiles } from "./AgentDirectory";
import { searchWiki, wikiSources } from "./wiki-content";
import { Icon } from "./ui";

export function WikiGeneratorDialog({
  record,
  workspace,
  onClose,
  onGenerate,
}) {
  const [settings, setSettings] = useState(record.settings);
  const patch = (change) => setSettings((s) => ({ ...s, ...change }));
  const valid =
    settings.runtime && (settings.runtime !== "Hermes" || settings.profile);
  return (
    <ProjectDialog
      title={record.pages.length ? "Update Wiki" : "Generate Wiki"}
      subtitle="Read a workspace snapshot and publish a checked set of pages."
      action="Start update"
      disabled={!valid}
      onClose={onClose}
      onSubmit={(e) => {
        e.preventDefault();
        if (valid) onGenerate(settings);
      }}
      footer="Simulated run · no runtime is launched"
    >
      <div className="wiki-dialog-scope">
        <Icon name="folder" />
        <div>
          <strong>{workspace.name}</strong>
          <code>{workspace.path}</code>
        </div>
        {workspace.kind === "git" && <span>{workspace.branch}</span>}
      </div>
      <label className="project-field">
        Wiki runtime
        <select
          value={settings.runtime}
          onChange={(e) =>
            patch({ runtime: e.target.value, model: "", profile: "" })
          }
        >
          <option value="">Choose a runtime</option>
          {sampleRuntimes.map((r) => (
            <option key={r}>{r}</option>
          ))}
        </select>
      </label>
      {settings.runtime === "Hermes" ? (
        <label className="project-field">
          Hermes profile
          <select
            aria-label="Hermes profile"
            value={settings.profile}
            onChange={(e) => patch({ profile: e.target.value })}
          >
            <option value="">Choose a profile</option>
            {sampleProfiles.map((p) => (
              <option key={p}>{p}</option>
            ))}
          </select>
          <small>Model and provider come from this profile.</small>
        </label>
      ) : (
        settings.runtime && (
          <label className="project-field">
            Wiki model <span className="optional">Optional</span>
            <input
              value={settings.model}
              placeholder="Runtime default"
              onChange={(e) => patch({ model: e.target.value })}
            />
          </label>
        )
      )}
      <p className="project-field-help">
        One temporary generation session. This setting is independent of thread
        recap and your team agents.
      </p>
      <ol className="wiki-generation-list">
        <li>Read the source snapshot</li>
        <li>Plan the table of contents</li>
        <li>Write the changed pages</li>
        <li>Check references, then publish the revision</li>
      </ol>
      <p className="project-field-help">
        Your current pages stay readable while an update runs.
      </p>
    </ProjectDialog>
  );
}

export function WikiSearchDialog({ pages, onSelect, onClose }) {
  const [query, setQuery] = useState("");
  const results = searchWiki(pages, query);
  return (
    <ProjectDialog
      title="Search Wiki"
      subtitle="Search titles and the full text of this snapshot."
      action="Done"
      onClose={onClose}
      onSubmit={(e) => {
        e.preventDefault();
        onClose();
      }}
    >
      <label className="project-field">
        Search this repository
        <input
          value={query}
          placeholder="Try canonical, lease, or source…"
          onChange={(e) => setQuery(e.target.value)}
        />
      </label>
      <div className="wiki-search-results" aria-live="polite">
        {results.length ? (
          results.map(({ page, excerpt }) => (
            <button
              type="button"
              key={page.id}
              onClick={() => onSelect(page.id)}
            >
              <strong>{page.title}</strong>
              <span>{excerpt}</span>
            </button>
          ))
        ) : (
          <p>No pages match “{query}”. Try a shorter phrase.</p>
        )}
      </div>
    </ProjectDialog>
  );
}

export function WikiHistoryDialog({ history, onSelect, onClose }) {
  return (
    <ProjectDialog
      title="Your Wiki questions"
      subtitle="Private to you in this preview session. Nothing has been posted to a channel."
      action="Done"
      onClose={onClose}
      onSubmit={(e) => {
        e.preventDefault();
        onClose();
      }}
    >
      <div className="wiki-search-results">
        {history.length ? (
          [...history].reverse().map((q) => (
            <button type="button" key={q.id} onClick={() => onSelect(q.id)}>
              <strong>{q.prompt}</strong>
              <span>
                {q.agent} · {q.state} · {q.revision.slice(0, 7)}
              </span>
            </button>
          ))
        ) : (
          <p>
            No questions yet. Ask about a page to start your private history.
          </p>
        )}
      </div>
    </ProjectDialog>
  );
}

export function WikiTaskDialog({
  record,
  question,
  project,
  agents,
  onClose,
  onSave,
  onStart,
}) {
  const initial = record.taskDraft || {
    title: `Follow up: ${question.prompt}`.slice(0, 120),
    prompt: `${question.prompt}\n\nContext from Wiki:\n${question.answer.paragraphs.map((p) => p.text).join("\n\n")}\n\nPlease propose an implementation plan and checks before making changes.`,
    channel:
      project.channels.find((c) => c.name === "engineering")?.name ||
      project.channels.find((c) => !c.shared)?.name ||
      "",
    agent: question.agent,
    questionId: question.id,
    citations: question.answer.sources,
  };
  const [draft, setDraft] = useState(initial);
  const patch = (change) => setDraft((d) => ({ ...d, ...change }));
  const channels = project.channels.filter((c) => !c.shared);
  const eligible = agents.filter((a) =>
    a.channels.includes(`${project.name}/${draft.channel}`),
  );
  const ready = eligible.some(
    (a) => a.id === draft.agent && a.status === "Available",
  );
  const valid =
    draft.title.trim() &&
    draft.prompt.trim() &&
    channels.some((c) => c.name === draft.channel);
  return (
    <ProjectDialog
      title="Create task draft"
      subtitle="Review what will be shared before starting a thread."
      action="Start thread"
      disabled={!valid || !ready}
      onClose={onClose}
      onSubmit={(e) => {
        e.preventDefault();
        if (valid && ready) onStart(draft);
      }}
      footer="Start thread creates a local sample only"
    >
      <label className="project-field">
        Task title
        <input
          value={draft.title}
          maxLength={120}
          onChange={(e) => patch({ title: e.target.value })}
        />
      </label>
      <div className="wiki-dialog-columns">
        <label className="project-field">
          Channel
          <select
            value={draft.channel}
            onChange={(e) => patch({ channel: e.target.value })}
          >
            {!channels.length && <option value="">No project channel</option>}
            {channels.map((c) => (
              <option key={c.name} value={c.name}>
                #{c.name}
              </option>
            ))}
          </select>
        </label>
        <label className="project-field">
          Task agent
          <select
            value={draft.agent}
            onChange={(e) => patch({ agent: e.target.value })}
          >
            {!eligible.some((a) => a.id === draft.agent) && (
              <option value={draft.agent}>{draft.agent} · unavailable</option>
            )}
            {eligible.map((a) => (
              <option
                key={a.id}
                value={a.id}
                disabled={a.status !== "Available"}
              >
                {a.name}
                {a.status !== "Available" ? ` · ${a.status.toLowerCase()}` : ""}
              </option>
            ))}
          </select>
        </label>
      </div>
      <label className="project-field">
        Task prompt
        <textarea
          rows={7}
          value={draft.prompt}
          onChange={(e) => patch({ prompt: e.target.value })}
        />
      </label>
      <div className="wiki-draft-sources">
        <strong>Included source references</strong>
        {draft.citations.map((id) => {
          const s = wikiSources.find((s) => s.id === id);
          return (
            <span key={id}>
              {s.path} · {s.start}–{s.end}
            </span>
          );
        })}
      </div>
      {!ready && (
        <p className="project-error">
          Choose an available agent who belongs to this channel.
        </p>
      )}
      <p className="project-field-help">
        Only this prompt and its references will be shared. Your other Wiki
        questions stay private.
      </p>
      <button
        type="button"
        className="project-button"
        disabled={!valid}
        onClick={() => onSave(draft)}
      >
        Save draft for later
      </button>
    </ProjectDialog>
  );
}
