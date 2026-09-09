import { useState } from "react";
import { Icon, IconButton, Avatar } from "./ui";
import { useAgentDirectory } from "./AgentDirectory";
export function Sidebar({
  attentionCount,
  selectedThread,
  screen,
  project,
  channel,
  go,
  agent,
  setAgent,
  setSearch,
  hide,
}) {
  const { agents } = useAgentDirectory();
  const [open, setOpen] = useState({
    NuncioCrew: true,
    HeardBack: true,
    Didit: false,
  });
  return (
    <aside className="sidebar" aria-label="Main navigation">
      <div className="brand">
        <span>NuncioCrew</span>
        <Icon name="down" size={13} />
        <div className="spacer" />
        <IconButton
          icon="search"
          label="Search conversations"
          onClick={() => setSearch(true)}
        />
        <IconButton icon="sidebar" label="Hide sidebar" onClick={hide} />
      </div>
      <nav className="global-nav" aria-label="Workspace">
        {["Inbox", "Wiki", "Agents", "Workflows"].map((n) => (
          <button
            key={n}
            className={`nav-row ${screen === n.toLowerCase() ? "selected" : ""}`}
            onClick={() => go(n.toLowerCase())}
          >
            <Icon name={n.toLowerCase()} />
            <span>{n}</span>
            {n === "Inbox" && <span className="count">{attentionCount}</span>}
          </button>
        ))}
      </nav>
      <section className="project-section">
        <div className="section-label">
          Projects <span className="muted-label">3</span>
        </div>
        <div className="project-scroll">
          {["NuncioCrew", "HeardBack", "Didit"].map((p) => (
            <div key={p} className="project-group">
              <button
                className="nav-row project-row"
                aria-expanded={open[p]}
                onClick={() => setOpen({ ...open, [p]: !open[p] })}
              >
                <Icon name={open[p] ? "down" : "right"} size={12} />
                <Icon name="folder" />
                <span>{p}</span>
                {p === "NuncioCrew" && <span className="tiny-dot green" />}
              </button>
              {open[p] && (
                <div className="project-children">
                  {(p === "NuncioCrew"
                    ? ["product", "engineering"]
                    : p === "HeardBack"
                      ? ["marketing", "customers", "general"]
                      : ["general"]
                  ).map((c) => (
                    <button
                      key={c}
                      data-film={`channel-${c}`}
                      className={`nav-row child ${project === p && channel === c && ["channel", "empty"].includes(screen) ? "selected" : ""}`}
                      onClick={() =>
                        go(c === "general" ? "empty" : "channel", {
                          project: p,
                          channel: c,
                        })
                      }
                    >
                      <Icon name="hash" size={14} />
                      {c}
                    </button>
                  ))}
                  {p === "NuncioCrew" && (
                    <>
                      <button
                        className={`nav-row thread-link child ${screen === "thread" && selectedThread === "workspace" ? "selected" : ""}`}
                        onClick={() => go("thread")}
                      >
                        <span className="truncate">Rebuild the workspace</span>
                        <span className="tiny-dot green" />
                      </button>
                    </>
                  )}
                </div>
              )}
            </div>
          ))}
        </div>
      </section>
      <section className="dm-section">
        <div className="section-label">Direct messages</div>
        {agents.map((a) => (
          <button
            key={a.id}
            className={`nav-row agent-row ${screen === "dm" && agent === a.id ? "selected" : ""}`}
            onClick={() => {
              setAgent(a.id);
              go("dm");
            }}
          >
            <Avatar name={a.id} small />
            <span>{a.name}</span>
            <span
              className={`tiny-dot ${a.name === "Hermes" ? "green" : "neutral"}`}
            />
          </button>
        ))}
      </section>
      <div className="user-row">
        <Avatar name="Oscar" small />
        <span>Oscar Le</span>
        <button
          className={`user-settings ${screen === "settings" ? "selected" : ""}`}
          aria-current={screen === "settings" ? "page" : undefined}
          onClick={() => go("settings")}
        >
          <Icon name="settings" size={13} />
          <span>Settings</span>
        </button>
      </div>
    </aside>
  );
}
