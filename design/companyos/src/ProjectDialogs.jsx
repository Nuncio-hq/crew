import { useEffect, useRef, useState } from "react";
import { Icon, IconButton } from "./ui";
import { commonChannels, folderSamples, useProjects } from "./ProjectModel";

export function ProjectDialog({
  title,
  subtitle,
  children,
  onClose,
  onSubmit,
  action,
  disabled,
  footer,
}) {
  const ref = useRef(null),
    previous = useRef(document.activeElement);
  useEffect(() => {
    ref.current.showModal();
    ref.current
      .querySelector("input:not([disabled]), select:not([disabled])")
      ?.focus();
    return () => {
      previous.current?.focus?.();
    };
  }, []);
  return (
    <dialog
      ref={ref}
      className="project-dialog"
      onKeyDown={(e) => e.stopPropagation()}
      aria-labelledby="project-dialog-title"
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
    >
      <form onSubmit={onSubmit}>
        <header>
          <div>
            <span className="eyebrow">NUNCIOCREW</span>
            <h2 id="project-dialog-title">{title}</h2>
            <p>{subtitle}</p>
          </div>
          <IconButton icon="close" label="Close dialog" onClick={onClose} />
        </header>
        <div className="project-dialog-body">{children}</div>
        <footer>
          <span>{footer || "Prototype · changes stay in this preview"}</span>
          <div>
            <button className="project-button" type="button" onClick={onClose}>
              Cancel
            </button>
            <button
              className="project-button primary"
              type="submit"
              disabled={disabled}
            >
              {action}
            </button>
          </div>
        </footer>
      </form>
    </dialog>
  );
}
export function FolderPicker({ value, onChange, optional = true }) {
  const [picking, setPicking] = useState(false);
  return (
    <div className="folder-field">
      <div className="project-field-label">
        Local folder {optional && <span>Optional</span>}
      </div>
      {value ? (
        <div className="selected-folder">
          <Icon name="folder" size={21} />
          <div>
            <strong>{value.name}</strong>
            <code>{value.path}</code>
          </div>
          <button
            type="button"
            className="project-text-button"
            onClick={() => setPicking(!picking)}
          >
            Change
          </button>
          <IconButton
            icon="close"
            label="Remove selected folder"
            onClick={() => onChange(null)}
          />
        </div>
      ) : (
        <button
          className="folder-choose"
          type="button"
          aria-expanded={picking}
          onClick={() => setPicking(!picking)}
        >
          <Icon name="folder" />
          <span>Choose a folder</span>
          <Icon name="plus" size={14} />
        </button>
      )}
      {picking && (
        <div className="sample-folders" aria-label="Sample folder picker">
          <span className="eyebrow">SAMPLE FOLDERS · NO FILES ARE READ</span>
          {folderSamples.map((folder) => (
            <button
              key={folder.id}
              type="button"
              onClick={() => {
                onChange(folder);
                setPicking(false);
              }}
            >
              <Icon name="folder" />
              <span>
                <strong>{folder.name}</strong>
                <small>
                  {folder.machine} · {folder.path}
                </small>
              </span>
              <span className="folder-kind">
                {folder.available
                  ? folder.kind === "git"
                    ? "Git"
                    : "Folder"
                  : "Unavailable"}
              </span>
            </button>
          ))}
        </div>
      )}
      {value ? (
        <div
          className={`folder-detection ${!value.available ? "unavailable" : ""}`}
          role="status"
        >
          <Icon name={value.available ? "check" : "alert"} />
          <div>
            <strong>
              {!value.available
                ? "Folder unavailable on this Mac"
                : value.kind === "git"
                  ? "Git repository detected"
                  : "Folder workspace"}
            </strong>
            <p>
              {!value.available
                ? "Choose an accessible local folder to continue. Linking a project does not copy files between machines."
                : value.kind === "git"
                  ? `${value.machine} · Branch ${value.branch} · ${value.changes ? `${value.changes} uncommitted changes` : "Working tree clean"}`
                  : "Use this folder for documents and other work. Git is not required."}
            </p>
          </div>
        </div>
      ) : (
        <p className="project-field-help">
          {optional
            ? "Start with conversations. You can link a workspace later."
            : "Choose the folder you want to link to this project."}
        </p>
      )}
      <p className="project-field-help">
        Folder selection and Git detection are simulated here.
      </p>
    </div>
  );
}
export function AddProjectDialog({ onClose, onCreated }) {
  const { projects, add } = useProjects();
  const [name, setName] = useState(""),
    [description, setDescription] = useState(""),
    [folder, setFolder] = useState(null),
    [mode, setMode] = useState("new"),
    [channel, setChannel] = useState("general");
  const duplicate = projects.some(
    (p) => p.name.toLowerCase() === name.trim().toLowerCase(),
  );
  const valid =
    name.trim() &&
    channel.trim() &&
    !duplicate &&
    (!folder || folder.available);
  return (
    <ProjectDialog
      title="Add project"
      subtitle="Bring your conversations and workspace together."
      action="Create project"
      disabled={!valid}
      onClose={onClose}
      onSubmit={(e) => {
        e.preventDefault();
        if (!valid) return;
        const next = {
          name: name.trim(),
          description: description.trim() || "A shared place for this project.",
          channels: [
            {
              name: channel.trim(),
              description:
                mode === "existing"
                  ? commonChannels.find((c) => c.name === channel)?.description
                  : "The main conversation for this project",
              home: true,
              shared: mode === "existing",
            },
          ],
          workspaces: folder ? [folder] : [],
        };
        add(next);
        onCreated(next);
      }}
    >
      <label className="project-field">
        Project name
        <input
          autoFocus
          maxLength={80}
          placeholder="e.g. NuncioCrew"
          value={name}
          aria-invalid={duplicate}
          onChange={(e) => setName(e.target.value)}
        />
      </label>
      {duplicate && (
        <p className="project-error" role="alert">
          A project with this name already exists.
        </p>
      )}
      <label className="project-field">
        Description <span className="optional">Optional</span>
        <input
          maxLength={200}
          placeholder="What are you working on?"
          value={description}
          onChange={(e) => setDescription(e.target.value)}
        />
      </label>
      <FolderPicker
        value={folder}
        onChange={(v) => {
          setFolder(v);
          if (!name && v) setName(v.name);
        }}
      />
      <div className="project-channel-field">
        <div className="project-field-label">Main channel</div>
        <div className="project-segment" aria-label="Main channel source">
          {[
            ["new", "Create new"],
            ["existing", "Link existing"],
          ].map(([id, label]) => (
            <button
              type="button"
              aria-pressed={mode === id}
              key={id}
              onClick={() => {
                setMode(id);
                setChannel("general");
              }}
            >
              {label}
            </button>
          ))}
        </div>
        {mode === "new" ? (
          <label className="project-field compact">
            <span className="sr-only">Main channel name</span>
            <span className="project-channel-input">
              <Icon name="hash" />
              <input
                aria-label="Main channel name"
                value={channel}
                maxLength={50}
                onChange={(e) =>
                  setChannel(
                    e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ""),
                  )
                }
              />
            </span>
          </label>
        ) : (
          <select
            aria-label="Existing main channel"
            value={channel}
            onChange={(e) => setChannel(e.target.value)}
          >
            {commonChannels.map((c) => (
              <option key={c.name} value={c.name}>
                #{c.name}
              </option>
            ))}
          </select>
        )}
        <p className="project-field-help">
          {mode === "new"
            ? "Add more channels once the project is created."
            : "Existing messages and membership stay with the channel."}
        </p>
      </div>
    </ProjectDialog>
  );
}
export function WorkspaceDialog({ project, current, onClose }) {
  const { setWorkspace } = useProjects();
  const [folder, setFolder] = useState(current || null);
  return (
    <ProjectDialog
      title={current ? "Manage workspace" : "Link a workspace"}
      subtitle={`A local folder for ${project}.`}
      action="Save workspace"
      disabled={!folder?.available}
      onClose={onClose}
      onSubmit={(e) => {
        e.preventDefault();
        if (!folder?.available) return;
        setWorkspace(project, folder, current?.id);
        onClose();
      }}
    >
      <FolderPicker value={folder} onChange={setFolder} optional={false} />
      <div className="project-info">
        <Icon name="info" />
        <p>
          The folder stays on its own machine. Each agent needs access to its
          working folder; linking it here does not move files or switch an
          active task.
        </p>
      </div>
      {current && (
        <button
          type="button"
          className="disconnect-workspace"
          onClick={() => {
            setWorkspace(project, null, current.id);
            onClose();
          }}
        >
          Unlink this workspace{" "}
          <span>Keep channels, files and Git history</span>
        </button>
      )}
    </ProjectDialog>
  );
}
export function ChannelDialog({ project, onClose }) {
  const { addChannel } = useProjects();
  const [mode, setMode] = useState("new"),
    [name, setName] = useState(""),
    [description, setDescription] = useState("");
  const choices = commonChannels.filter(
    (c) => !project.channels.some((p) => p.name === c.name),
  );
  const exists = project.channels.some((c) => c.name === name.trim());
  const valid = name.trim() && !exists;
  return (
    <ProjectDialog
      title="Add channel"
      subtitle={`A conversation space in ${project.name}.`}
      action={mode === "new" ? "Create channel" : "Link channel"}
      disabled={!valid}
      onClose={onClose}
      onSubmit={(e) => {
        e.preventDefault();
        if (!valid) return;
        addChannel(project.name, {
          name: name.trim(),
          description:
            mode === "existing"
              ? choices.find((c) => c.name === name)?.description
              : description.trim() || "A shared conversation",
          shared: mode === "existing",
        });
        onClose();
      }}
    >
      <div className="project-segment">
        {[
          ["new", "Create new"],
          ["existing", "Link existing"],
        ].map(([id, label]) => (
          <button
            type="button"
            key={id}
            aria-pressed={mode === id}
            onClick={() => {
              setMode(id);
              setName(id === "existing" ? choices[0]?.name || "" : "");
            }}
          >
            {label}
          </button>
        ))}
      </div>
      {mode === "new" ? (
        <>
          <label className="project-field">
            Channel name
            <input
              autoFocus
              placeholder="e.g. design"
              value={name}
              maxLength={50}
              onChange={(e) =>
                setName(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ""))
              }
            />
          </label>
          <label className="project-field">
            Description <span className="optional">Optional</span>
            <input
              value={description}
              maxLength={200}
              placeholder="What belongs here?"
              onChange={(e) => setDescription(e.target.value)}
            />
          </label>
        </>
      ) : (
        <label className="project-field">
          Existing channel
          <select value={name} onChange={(e) => setName(e.target.value)}>
            {choices.length ? (
              choices.map((c) => (
                <option key={c.name} value={c.name}>
                  #{c.name}
                </option>
              ))
            ) : (
              <option value="">No available channels</option>
            )}
          </select>
        </label>
      )}
      {exists && (
        <p className="project-error" role="alert">
          This project already has a channel with that name.
        </p>
      )}
      <p className="project-field-help">
        {mode === "new"
          ? "The channel will appear under this project."
          : "Linking keeps existing messages and membership intact."}
      </p>
    </ProjectDialog>
  );
}
