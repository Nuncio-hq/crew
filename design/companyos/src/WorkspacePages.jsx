import { useState } from "react";
import { Icon, Avatar } from "./ui";
import { needsAttention } from "./thread-model";
import { AgentDelete } from "./AgentDelete";
import { RecapSettings } from "./RecapSettings";
import { AgentEditor } from "./AgentEditor";
import { useAgentDirectory } from "./AgentDirectory";
export function WorkspacePages({
  screen,
  go,
  load,
  setAgent,
  threads,
  openThread,
}) {
  const { agents } = useAgentDirectory();
  const [filter, setFilter] = useState("All");
  const [query, setQuery] = useState("");
  const [paused, setPaused] = useState(false);
  const titles = {
    inbox: ["Your inbox", "The next thing that needs your attention."],
    agents: [
      "Your agents",
      "People to think with, build with, and hand work to.",
    ],
    settings: ["Settings", "Choose how Crew helps you follow the work."],
    workflows: ["Workflows", "The routines that keep work moving."],
  };
  const [title, description] = titles[screen];
  return (
    <section className="workspace-page">
      <header>
        <Icon name={screen} />
        <span>{title}</span>
      </header>
      <div className="workspace-body">
        <span className="eyebrow">PERSONAL WORKSPACE</span>
        <h1>{title}</h1>
        <p className="page-description">{description}</p>
        {screen === "inbox" && (
          <>
            <div className="filter-tabs">
              {["All", "Needs you", "Ready for review"].map((f) => (
                <button
                  key={f}
                  className={filter === f ? "active" : ""}
                  onClick={() => setFilter(f)}
                >
                  {f}
                </button>
              ))}
            </div>
            {threads
              .filter(needsAttention)
              .filter(
                (t) =>
                  filter === "All" ||
                  (filter === "Needs you"
                    ? t.status === "needs-you"
                    : t.status === "done"),
              )
              .map((t) => (
                <button
                  className="inbox-item"
                  key={t.id}
                  onClick={() => openThread(t.id)}
                >
                  <Icon name="info" />
                  <div>
                    <strong>{t.title}</strong>
                    <p>
                      {t.owner} · {t.project || "NuncioCrew"} / #
                      {t.channel || "product"}
                    </p>
                  </div>
                  <span className="pill">
                    {t.status === "done" ? "Ready for review" : "Needs you"}
                  </span>
                  <Icon name="right" />
                </button>
              ))}
            <p className="quiet-note">
              The same attention items appear in their channel.
            </p>
          </>
        )}
        {screen === "agents" && (
          <>
            <div className="directory-toolbar">
              <span>{agents.length} agents</span>
              <AgentEditor />
            </div>
            <label className="search-field">
              <Icon name="search" />
              <input
                placeholder="Find an agent…"
                aria-label="Find an agent"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </label>
            <div className="directory">
              {agents
                .filter((a) =>
                  (a.name + " " + a.runtime + " " + a.profile + " " + a.model)
                    .toLowerCase()
                    .includes(query.toLowerCase()),
                )
                .map((a) => (
                  <div className="agent-card" key={a.id}>
                    <Avatar name={a.id} />
                    <h2>{a.name}</h2>
                    <p>
                      {a.runtime}
                      {a.runtime === "Hermes"
                        ? ` · Profile: ${a.profile}`
                        : ` · Model: ${a.model || "Runtime default"}`}
                    </p>
                    <small className="agent-memberships">
                      {a.channels.length
                        ? `${a.channels.length} channels`
                        : "No channels yet"}
                    </small>
                    <span className="muted">{a.status}</span>
                    <div className="agent-card-actions">
                      <AgentEditor agent={a} />
                      <AgentDelete agent={a} />
                      <button
                        onClick={() => {
                          setAgent(a.id);
                          go("dm");
                        }}
                      >
                        Message <Icon name="message" size={14} />
                      </button>
                    </div>
                  </div>
                ))}
            </div>
            {!agents.some((a) =>
              (a.name + " " + a.runtime + " " + a.profile + " " + a.model)
                .toLowerCase()
                .includes(query.toLowerCase()),
            ) && <p>No agents match “{query}”.</p>}
          </>
        )}
        {screen === "settings" && <RecapSettings />}
        {screen === "workflows" && (
          <div className="workflow-card">
            <div>
              <Icon name="workflows" size={25} />
              <h2>Weekly project review</h2>
              <p>Collect progress and questions for the founder.</p>
              <span className="pill">
                {paused ? "Paused" : "Every Monday · 09:00"}
              </span>
            </div>
            <button className="light-button" onClick={() => setPaused(!paused)}>
              <Icon name={paused ? "play" : "stop"} size={14} />
              {paused ? "Resume" : "Pause"} example
            </button>
            <p className="quiet-note">
              Local demonstration only. No schedule is created or changed.
            </p>
          </div>
        )}
      </div>
    </section>
  );
}
