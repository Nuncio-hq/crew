import { useState } from "react";
import { Icon } from "./ui";
import { commonChannels, useProjects } from "./ProjectModel";
import { ChannelDialog, WorkspaceDialog } from "./ProjectDialogs";
import "./projects.css";

export function ProjectPage({ projectName, go, threads, shared = false }) {
  const { projects } = useProjects();
  const project = projects.find((p) => p.name === projectName) || projects[0];
  const [dialog, setDialog] = useState(null);
  const channels = shared
    ? commonChannels.filter(
        (c) =>
          !projects.some((p) =>
            p.channels.some((ch) => ch.shared && ch.name === c.name),
          ),
      )
    : project.channels;
  return (
    <section
      className="project-page"
      aria-label={shared ? "Workspace channels" : `${project.name} project`}
    >
      <header className="project-page-header">
        <div className="title-line">
          <Icon name={shared ? "hash" : "folder"} />
          <span className="breadcrumb">
            {shared ? "Workspace" : "Projects"}
          </span>
          <Icon name="right" size={12} />
          <span>{shared ? "Channels" : project.name}</span>
        </div>
      </header>
      <div className="project-page-scroll">
        <div className="project-page-content">
          <div className="project-intro">
            <div className="project-title-icon">
              <Icon name={shared ? "hash" : "folder"} size={25} />
            </div>
            <span className="eyebrow">{shared ? "WORKSPACE" : "PROJECT"}</span>
            <h1>{shared ? "Channels" : project.name}</h1>
            <p>
              {shared
                ? "Shared conversations beyond any one project."
                : project.description}
            </p>
            {!shared && (
              <div className="project-meta">
                <button
                  className="project-text-button"
                  onClick={() => go("wiki", { project: project.name })}
                >
                  <Icon name="wiki" size={13} /> Open Wiki
                </button>
                <span>
                  <Icon name="hash" size={13} />
                  {channels.length} channels
                </span>
                <span>
                  <Icon name="folder" size={13} />
                  {project.workspaces.length} workspace
                  {project.workspaces.length !== 1 ? "s" : ""}
                </span>
                <span>
                  <Icon name="agents" size={13} />
                  Oscar Le · owner
                </span>
              </div>
            )}
          </div>
          <section
            className="project-block"
            aria-labelledby="project-channels-heading"
          >
            <header>
              <div>
                <h2 id="project-channels-heading">Channels</h2>
                <p>
                  {shared
                    ? "Choose a channel to join the conversation."
                    : "Where conversations turn into work."}
                </p>
              </div>
              {!shared && (
                <button
                  className="project-button"
                  onClick={() => setDialog({ type: "channel" })}
                >
                  <Icon name="plus" size={14} />
                  Add channel
                </button>
              )}
            </header>
            <div className="project-channel-list">
              {channels.map((c) => {
                const matches = shared
                  ? []
                  : threads.filter(
                      (t) =>
                        (t.project || "NuncioCrew") === project.name &&
                        (t.channel || "product") === c.name,
                    );
                const active = matches.filter(
                  (t) => t.status === "working",
                ).length;
                return (
                  <button
                    className="project-channel-row"
                    key={c.name}
                    onClick={() =>
                      go(matches.length ? "channel" : "empty", {
                        project:
                          shared || c.shared ? "Workspace" : project.name,
                        channel: c.name,
                      })
                    }
                  >
                    <span className="project-channel-icon">
                      <Icon name="hash" size={19} />
                    </span>
                    <span className="project-channel-copy">
                      <span>
                        <strong>{c.name}</strong>
                        {c.home && (
                          <small className="home-label">Main channel</small>
                        )}
                      </span>
                      <small>{c.description}</small>
                    </span>
                    <span className="project-channel-state">
                      {active ? (
                        <>
                          <span className="tiny-dot green" />
                          {active} in progress
                        </>
                      ) : matches.length ? (
                        `${matches.length} threads`
                      ) : (
                        "Start a conversation"
                      )}
                    </span>
                    <Icon name="right" size={15} />
                  </button>
                );
              })}
              {!channels.length && (
                <p className="project-empty-note">
                  All shared channels are linked under Projects.
                </p>
              )}
            </div>
          </section>
          {!shared && (
            <section
              className="project-block"
              aria-labelledby="project-workspace-heading"
            >
              <header>
                <div>
                  <h2 id="project-workspace-heading">Workspace</h2>
                  <p>Folders and repositories connected to this project.</p>
                </div>
                <button
                  className="project-button"
                  onClick={() => setDialog({ type: "workspace" })}
                >
                  <Icon name="plus" size={14} />
                  Link folder
                </button>
              </header>
              {project.workspaces.length ? (
                <div className="project-workspace-list">
                  {project.workspaces.map((w) => (
                    <article className="workspace-card" key={w.id}>
                      <div className="workspace-card-top">
                        <span className="workspace-folder-icon">
                          <Icon name="folder" size={23} />
                        </span>
                        <div>
                          <h3>{w.name}</h3>
                          <span>
                            {w.kind === "git"
                              ? "Git repository"
                              : "Folder workspace"}
                          </span>
                        </div>
                        <div className="spacer" />
                        <button
                          className="project-button quiet"
                          aria-label={`Manage workspace ${w.name}`}
                          onClick={() =>
                            setDialog({ type: "workspace", current: w })
                          }
                        >
                          Manage
                          <Icon name="right" size={12} />
                        </button>
                      </div>
                      <code className="workspace-path">{w.path}</code>
                      <div className="workspace-card-bottom">
                        <span>
                          <Icon name="terminal" size={13} />
                          {w.machine}
                        </span>
                        {w.kind === "git" && (
                          <span>
                            <Icon name="branch" size={13} />
                            {w.branch}
                          </span>
                        )}
                        <span className="workspace-available">
                          <span className="tiny-dot green" />
                          Linked · sample
                        </span>
                      </div>
                    </article>
                  ))}
                </div>
              ) : (
                <div className="project-empty-workspace">
                  <Icon name="folder" size={26} />
                  <h3>No workspace linked yet</h3>
                  <p>
                    Your channels are ready. Link a folder when you want agents
                    to work with files.
                  </p>
                  <button
                    className="project-button"
                    onClick={() => setDialog({ type: "workspace" })}
                  >
                    Choose a folder
                    <Icon name="right" size={14} />
                  </button>
                </div>
              )}
              <p className="workspace-scope-note">
                <Icon name="info" size={14} />
                Folders stay on their own machine. Agents need access to that
                machine to use them.
              </p>
            </section>
          )}
        </div>
      </div>
      {dialog?.type === "channel" && (
        <ChannelDialog project={project} onClose={() => setDialog(null)} />
      )}
      {dialog?.type === "workspace" && (
        <WorkspaceDialog
          project={project.name}
          current={dialog.current}
          onClose={() => setDialog(null)}
        />
      )}
    </section>
  );
}
