import { createContext, useContext, useEffect, useRef, useState } from "react";
import { channelTeam } from "./thread-model";
import { Avatar, Icon } from "./ui";
import { useAgentDirectory } from "./AgentDirectory";
const RoleContext = createContext(null);
const scopeKey = (project, channel) =>
  JSON.stringify([project || "NuncioCrew", channel || "product"]);
function initialRoles(project, channel) {
  return channelTeam(project, channel).map((m, i) => ({
    id: `seed-${i}`,
    role: m.role,
    scope: m.scope,
    holders: [m.name],
  }));
}
export function ChannelRolesProvider({ children }) {
  const [catalogs, setCatalogs] = useState({});
  return (
    <RoleContext.Provider
      value={{
        get: (project, channel) =>
          catalogs[scopeKey(project, channel)] ?? {
            roles: initialRoles(project, channel),
            contactPoint: null,
          },
        save: (project, channel, roles, contactPoint) =>
          setCatalogs((current) => ({
            ...current,
            [scopeKey(project, channel)]: {
              contactPoint,
              roles: roles.map((r) => ({
                ...r,
                holders: [...r.holders],
              })),
            },
          })),
        reset: () => setCatalogs({}),
      }}
    >
      {children}
    </RoleContext.Provider>
  );
}
export function useChannelRoles(project, channel) {
  const store = useContext(RoleContext);
  const { agents } = useAgentDirectory();
  const activeIds = new Set(agents.map((a) => a.id));
  const snapshot = store.get(project, channel);
  const roles = snapshot.roles.map((r) => ({
    ...r,
    holders: r.holders.filter((id) => activeIds.has(id)),
  }));
  const contactPoint = activeIds.has(snapshot.contactPoint)
    ? snapshot.contactPoint
    : null;
  const team = roles.flatMap((r) =>
    r.holders.map((id) => ({ name: id, role: r.role, scope: r.scope })),
  );
  return {
    roles,
    contactPoint,
    team,
    save: (rows, contact) => store.save(project, channel, rows, contact),
    reset: store.reset,
  };
}
export function ChannelRoleEditor({ project, channel }) {
  const { roles, contactPoint, save } = useChannelRoles(project, channel);
  const { agents } = useAgentDirectory();
  const members = agents.filter((a) =>
    a.channels.includes(`${project}/${channel}`),
  );
  const dialog = useRef(null),
    trigger = useRef(null);
  const [rows, setRows] = useState([]),
    [draftContact, setDraftContact] = useState(""),
    [error, setError] = useState(""),
    [saved, setSaved] = useState(false),
    [focusId, setFocusId] = useState(null);
  useEffect(() => {
    if (focusId)
      dialog.current
        ?.querySelector(`[data-role-id="${focusId}"] input`)
        ?.focus();
  }, [focusId]);
  function open() {
    setRows(roles.map((r) => ({ ...r, holders: [...r.holders] })));
    setDraftContact(contactPoint || "");
    setError("");
    setSaved(false);
    dialog.current.showModal();
  }
  function close() {
    dialog.current.close();
    trigger.current?.focus();
  }
  function update(id, patch) {
    setError("");
    setRows((current) =>
      current.map((r) => (r.id === id ? { ...r, ...patch } : r)),
    );
  }
  function add() {
    const id = crypto.randomUUID();
    setRows((current) => [
      ...current,
      { id, role: "", scope: "", holders: [] },
    ]);
    setFocusId(id);
    setError("");
  }
  function assign(roleId, agentId, checked) {
    setRows((current) =>
      current.map((r) => ({
        ...r,
        holders:
          r.id === roleId && checked
            ? [...r.holders.filter((id) => id !== agentId), agentId]
            : r.holders.filter((id) => id !== agentId),
      })),
    );
  }
  function commit(e) {
    e.preventDefault();
    const next = rows.map((r) => ({
      ...r,
      role: r.role.trim(),
      scope: r.scope.trim(),
    }));
    if (next.some((r) => !r.role || !r.scope)) {
      setError("Give every role a name and responsibility definition.");
      return;
    }
    if (new Set(next.map((r) => r.role.toLowerCase())).size !== next.length) {
      setError(
        "Use a unique name for each role. Multiple agents can hold one role.",
      );
      return;
    }
    if (draftContact && !members.some((a) => a.id === draftContact)) {
      setError(
        "Choose a current channel member as contact point, or select None.",
      );
      return;
    }
    save(next, draftContact || null);
    setSaved(true);
    close();
  }
  return (
    <div className="role-editor-control">
      <button ref={trigger} className="assign-roles" onClick={open}>
        <Icon name="agents" size={14} />
        Assign roles
      </button>
      {saved && (
        <span role="status" className="role-save-note">
          Saved for #{channel} · sample
        </span>
      )}
      <dialog
        ref={dialog}
        className="channel-role-dialog"
        aria-labelledby="channel-role-title"
      >
        <form onSubmit={commit}>
          <header>
            <div>
              <span className="eyebrow">
                {project} / #{channel}
              </span>
              <h2 id="channel-role-title">Channel roles</h2>
            </div>
            <button
              type="button"
              onClick={close}
              aria-label="Close role editor"
            >
              <Icon name="close" />
            </button>
          </header>
          <p className="role-editor-intro">
            Define the roles this channel needs, then choose who holds them.
            Each agent holds one role here.
          </p>
          <div className="channel-contact-field">
            <label htmlFor="channel-contact-point">Channel contact point</label>
            <select
              id="channel-contact-point"
              value={draftContact}
              aria-describedby="channel-contact-help"
              onChange={(e) => {
                setDraftContact(e.target.value);
                setError("");
              }}
            >
              <option value="">None — mention an agent to get a reply</option>
              {draftContact && !members.some((a) => a.id === draftContact) && (
                <option value={draftContact} disabled>
                  Unavailable member
                </option>
              )}
              {members.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
            </select>
            <p id="channel-contact-help" className="field-help">
              Replies to your messages in this channel and its threads when you
              don’t @mention anyone. Explicit @mentions take priority. Agent
              messages do not trigger the contact point.
            </p>
          </div>
          <div className="role-add-toolbar">
            <span>{rows.length} roles</span>
            <button type="button" className="light-button" onClick={add}>
              <Icon name="plus" size={14} />
              Add role
            </button>
          </div>
          <div className="role-editor-fields">
            {!rows.length && (
              <p>No roles yet. Add the first role for this channel.</p>
            )}
            {rows.map((r, i) => (
              <fieldset key={r.id} data-role-id={r.id}>
                <legend>
                  {r.role || "New role"}
                  {!r.holders.length && (
                    <span className="unassigned-role">Unassigned</span>
                  )}
                </legend>
                <label>
                  Role name {i + 1}
                  <input
                    value={r.role}
                    maxLength={128}
                    placeholder="e.g. QA reviewer"
                    onChange={(e) => update(r.id, { role: e.target.value })}
                  />
                </label>
                <label>
                  Responsibilities {i + 1}
                  <textarea
                    value={r.scope}
                    rows={2}
                    placeholder="Allowed work, limits, and when to hand off…"
                    onChange={(e) => update(r.id, { scope: e.target.value })}
                  />
                </label>
                <div
                  className="role-holder-picks"
                  role="group"
                  aria-label={`Agents for ${r.role || "new role"}`}
                >
                  <span>Assign to</span>
                  {members.map((a) => (
                    <label key={a.id}>
                      <input
                        type="checkbox"
                        checked={r.holders.includes(a.id)}
                        onChange={(e) => assign(r.id, a.id, e.target.checked)}
                      />
                      <Avatar name={a.id} small />
                      {a.name}
                    </label>
                  ))}
                </div>
                <p className="field-help">
                  {members.length
                    ? "Selecting an agent moves their assignment from any other role in this channel."
                    : "No agent members yet. Add an agent to this channel from Agents."}
                </p>
                <button
                  type="button"
                  className="remove-role"
                  aria-label={`Remove ${r.role || "new role"}`}
                  onClick={() =>
                    setRows((current) =>
                      current.filter((row) => row.id !== r.id),
                    )
                  }
                >
                  Remove role
                </button>
              </fieldset>
            ))}
          </div>
          {error && (
            <p className="role-error" role="alert">
              {error}
            </p>
          )}
          <footer>
            <p>
              Owner editing · simulated. Unassigned roles are kept. Tool access
              is configured separately.
            </p>
            <div>
              <button type="button" onClick={close}>
                Cancel
              </button>
              <button type="submit" className="light-button">
                Save roles
              </button>
            </div>
          </footer>
        </form>
      </dialog>
    </div>
  );
}
