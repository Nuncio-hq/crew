import { useRef, useState } from "react";
import { Icon } from "./ui";
import {
  sampleChannels,
  sampleProfiles,
  useAgentDirectory,
} from "./AgentDirectory";
export function AgentEditor({ agent }) {
  const { agents, save } = useAgentDirectory();
  const dialog = useRef(null),
    trigger = useRef(null);
  const [draft, setDraft] = useState(null),
    [error, setError] = useState("");
  function open() {
    setDraft(
      agent
        ? { ...agent, channels: [...agent.channels] }
        : {
            name: "",
            runtime: "Hermes",
            profile: "",
            model: "",
            channels: [],
            status: "Not started",
          },
    );
    setError("");
    dialog.current.showModal();
  }
  function close() {
    dialog.current.close();
    trigger.current?.focus();
  }
  function patch(update) {
    setDraft((d) => ({ ...d, ...update }));
    setError("");
  }
  function commit(e) {
    e.preventDefault();
    const name = draft.name.trim();
    if (!name) {
      setError("Enter a name for this agent.");
      return;
    }
    if (
      agents.some(
        (a) =>
          a.id !== agent?.id && a.name.toLowerCase() === name.toLowerCase(),
      )
    ) {
      setError("An agent already uses this name. Choose a different name.");
      return;
    }
    if (draft.runtime === "Hermes" && !draft.profile) {
      setError("Choose a Hermes profile.");
      return;
    }
    save({
      ...draft,
      name,
      id: agent?.id || crypto.randomUUID(),
      model: draft.runtime === "Hermes" ? "" : draft.model.trim(),
      profile: draft.runtime === "Hermes" ? draft.profile : "",
    });
    close();
  }
  return (
    <>
      <button
        ref={trigger}
        className={agent ? "agent-edit-button" : "light-button"}
        aria-label={agent ? `Edit ${agent.name}` : "Add agent"}
        onClick={open}
      >
        <Icon name={agent ? "settings" : "plus"} size={14} />
        {agent ? "Edit" : "Add agent"}
      </button>
      <dialog
        ref={dialog}
        className="channel-role-dialog agent-editor-dialog"
        aria-labelledby={`agent-editor-${agent?.id || "new"}`}
      >
        {draft && (
          <form onSubmit={commit}>
            <header>
              <div>
                <span className="eyebrow">WORKSPACE AGENT</span>
                <h2 id={`agent-editor-${agent?.id || "new"}`}>
                  {agent ? `Edit ${agent.name}` : "Add agent"}
                </h2>
              </div>
              <button
                type="button"
                onClick={close}
                aria-label="Close agent editor"
              >
                <Icon name="close" />
              </button>
            </header>
            <p className="role-editor-intro">
              Choose the runtime this colleague will use. Channel roles are
              assigned separately.
            </p>
            <div className="role-editor-fields">
              <label>
                Agent name
                <input
                  value={draft.name}
                  maxLength={80}
                  autoComplete="off"
                  placeholder="e.g. Campaign researcher"
                  onChange={(e) => patch({ name: e.target.value })}
                />
              </label>
              <label>
                Runtime
                <select
                  value={draft.runtime}
                  onChange={(e) =>
                    patch({ runtime: e.target.value, model: "", profile: "" })
                  }
                >
                  <option>Hermes</option>
                  <option>Codex</option>
                  <option>Claude</option>
                </select>
              </label>
              {draft.runtime === "Hermes" ? (
                <>
                  <label>
                    Hermes profile
                    <select
                      value={draft.profile}
                      onChange={(e) => patch({ profile: e.target.value })}
                    >
                      <option value="">Choose a profile…</option>
                      {sampleProfiles.map((p) => (
                        <option key={p}>{p}</option>
                      ))}
                    </select>
                  </label>
                  <p className="field-help">
                    Model, provider and tools come from this Hermes profile.
                    Manage them in Hermes. These profile names are examples.
                  </p>
                </>
              ) : (
                <>
                  <label>
                    Model ID
                    <input
                      value={draft.model}
                      autoComplete="off"
                      placeholder="Runtime default"
                      onChange={(e) => patch({ model: e.target.value })}
                    />
                  </label>
                  <p className="field-help">
                    Leave blank to use the runtime default, or enter a model ID
                    supported by this runtime. Saving does not verify model
                    access.
                  </p>
                </>
              )}
              {!agent ? (
                <fieldset className="agent-channel-picks">
                  <legend>Add to channels</legend>
                  <p className="field-help">
                    Optional. Choose where this agent will collaborate.
                  </p>
                  {sampleChannels.map((c) => (
                    <label key={c.id}>
                      <input
                        type="checkbox"
                        checked={draft.channels.includes(c.id)}
                        onChange={(e) =>
                          patch({
                            channels: e.target.checked
                              ? [...draft.channels, c.id]
                              : draft.channels.filter((id) => id !== c.id),
                          })
                        }
                      />
                      {c.label}
                    </label>
                  ))}
                </fieldset>
              ) : (
                <p className="field-help">
                  Configuration applies to future runs. The current run is
                  unchanged in this simulation.
                </p>
              )}
            </div>
            {error && (
              <p className="role-error" role="alert">
                {error}
              </p>
            )}
            <footer>
              <p>
                Simulated configuration · no runtime starts, profile changes or
                relay writes.
              </p>
              <div>
                <button type="button" onClick={close}>
                  Cancel
                </button>
                <button type="submit" className="light-button">
                  {agent ? "Save changes" : "Add agent"}
                </button>
              </div>
            </footer>
          </form>
        )}
      </dialog>
    </>
  );
}
