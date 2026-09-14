import { useRef } from "react";
import { useAgentDirectory } from "./AgentDirectory";
export function AgentDelete({ agent }) {
  const { remove } = useAgentDirectory();
  const dialog = useRef(null),
    trigger = useRef(null);
  const close = () => {
    dialog.current.close();
    trigger.current?.focus();
  };
  return (
    <>
      <button
        ref={trigger}
        className="agent-delete"
        aria-label={`Delete ${agent.name}`}
        onClick={() => dialog.current.showModal()}
      >
        Delete
      </button>
      <dialog
        ref={dialog}
        className="channel-role-dialog delete-agent-dialog"
        aria-labelledby={`delete-${agent.id}`}
      >
        <form
          onSubmit={(e) => {
            e.preventDefault();
            close();
            remove(agent.id);
            requestAnimationFrame(() =>
              document.querySelector('[aria-label="Add agent"]')?.focus(),
            );
          }}
        >
          <header>
            <h2 id={`delete-${agent.id}`}>Delete {agent.name}?</h2>
          </header>
          <div className="role-editor-intro">
            <p>
              Remove this agent from your directory, direct-message list and
              channel assignments. Any channel contact point using this agent
              will be cleared.
            </p>
            {agent.status === "Working" && (
              <p>
                This agent is working. Deleting it also stops its sample run.
              </p>
            )}
            <p>
              Existing conversations and authors stay in history. Runtime
              installations, profiles and worktrees are kept.
            </p>
            <p className="muted">
              Prototype only · Reset restores sample agents.
            </p>
          </div>
          <footer>
            <button type="button" onClick={close}>
              Cancel
            </button>
            <button className="danger-button" type="submit">
              Delete agent
            </button>
          </footer>
        </form>
      </dialog>
    </>
  );
}
