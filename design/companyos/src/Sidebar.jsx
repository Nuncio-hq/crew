import { commonChannels, useProjects } from "./ProjectModel";
import { useRef, useState } from "react";
import { Icon, IconButton, Avatar } from "./ui";
import { ProjectDialog } from "./ProjectDialogs";
import { useAgentDirectory } from "./AgentDirectory";
export function Sidebar({
  onAddProject,
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
  const { projects } = useProjects();
  const workspaceMenu = useRef(null);
  const [browsing, setBrowsing] = useState(false);
  function openCommonChannel(name) {
    setBrowsing(false);
    go("empty", { project: "Workspace", channel: name });
  }
  const { agents } = useAgentDirectory();
  const [open, setOpen] = useState({
    NuncioCrew: true,
    HeardBack: true,
    Didit: false,
  });
  return (
    <aside className="sidebar" aria-label="Main navigation">
      <div className="brand">
        <button
          className="workspace-menu-trigger"
          popoverTarget="workspace-menu"
          aria-label="Open workspace menu"
        >
          <span>NuncioCrew</span>
          <Icon name="down" size={13} />
        </button>
        <div
          ref={workspaceMenu}
          id="workspace-menu"
          popover="auto"
          className="workspace-menu"
        >
          <button
            className="nav-row"
            onClick={() => {
              workspaceMenu.current.hidePopover();
              setBrowsing(true);
            }}
          >
            <Icon name="hash" size={15} /> Browse channels
          </button>
          <button
            className="nav-row"
            onClick={() => {
              workspaceMenu.current.hidePopover();
              go("company-wiki");
            }}
          >
            <Icon name="wiki" size={15} /> Company Wiki
          </button>
        </div>
        <div className="spacer" />
        <IconButton
          icon="search"
          label="Search conversations"
          onClick={() => setSearch(true)}
        />
        <IconButton icon="sidebar" label="Hide sidebar" onClick={hide} />
      </div>
      <nav className="global-nav" aria-label="Workspace">
        {["Inbox", "Agents", "Workflows"].map((n) => (
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
          Projects <span className="muted-label">{projects.length}</span>
          <IconButton icon="plus" label="Add project" onClick={onAddProject} />
        </div>
        <div className="project-scroll">
          {projects.map((item) => {
            const p = item.name;
            return (
              <div key={p} className="project-group">
                <div
                  className={`project-nav-line ${screen === "project" && project === p ? "selected" : ""}`}
                >
                  <button
                    className="project-expand"
                    aria-label={`${open[p] ? "Collapse" : "Expand"} ${p} navigation`}
                    aria-expanded={!!open[p]}
                    onClick={() => setOpen({ ...open, [p]: !open[p] })}
                  >
                    <Icon name={open[p] ? "down" : "right"} size={12} />
                  </button>
                  <button
                    className="nav-row project-row"
                    aria-current={
                      screen === "project" && project === p ? "page" : undefined
                    }
                    onClick={() => go("project", { project: p })}
                  >
                    <Icon name="folder" />
                    <span>{p}</span>
                    {p === "NuncioCrew" && <span className="tiny-dot green" />}
                  </button>
                </div>
                {open[p] && (
                  <div className="project-children">
                    <button
                      className={`nav-row child ${screen === "wiki" && project === p ? "selected" : ""}`}
                      aria-label={`${p} Wiki`}
                      aria-current={
                        screen === "wiki" && project === p ? "page" : undefined
                      }
                      onClick={() => go("wiki", { project: p })}
                    >
                      <Icon name="wiki" size={14} />
                      Wiki
                    </button>
                    {item.channels.map(({ name: c, shared }) => (
                      <button
                        key={c}
                        data-film={`channel-${c}`}
                        className={`nav-row child ${project === (shared ? "Workspace" : p) && channel === c && ["channel", "empty"].includes(screen) ? "selected" : ""}`}
                        onClick={() =>
                          go(c === "general" ? "empty" : "channel", {
                            project: shared ? "Workspace" : p,
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
                          <span className="truncate">
                            Rebuild the workspace
                          </span>
                          <span className="tiny-dot green" />
                        </button>
                      </>
                    )}
                  </div>
                )}
              </div>
            );
          })}
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
      {browsing && (
        <ProjectDialog
          title="Browse channels"
          subtitle="Open a shared channel, including those linked to a project."
          onClose={() => setBrowsing(false)}
          onSubmit={(e) => {
            e.preventDefault();
            setBrowsing(false);
          }}
          action="Done"
          footer="Prototype · sample channels"
        >
          <div className="browse-channel-list">
            {commonChannels.map((c) => (
              <button
                key={c.name}
                type="button"
                className="project-channel-row"
                onClick={() => openCommonChannel(c.name)}
              >
                <Icon name="hash" size={17} />
                <span className="project-channel-copy">
                  <strong>{c.name}</strong>
                  <small>{c.description}</small>
                </span>
                <Icon name="right" size={13} />
              </button>
            ))}
          </div>
        </ProjectDialog>
      )}
    </aside>
  );
}
