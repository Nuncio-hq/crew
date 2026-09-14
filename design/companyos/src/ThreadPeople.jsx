import { useAgentDirectory } from "./AgentDirectory";
import { useState } from "react";
import { Avatar, Icon } from "./ui";
import { useChannelRoles } from "./ChannelRoles";
import { threadTeam } from "./thread-model";
export function ThreadPeople({
  thread,
  project,
  channel,
  expandedByDefault = false,
}) {
  const { displayName } = useAgentDirectory();
  const [expanded, setExpanded] = useState(expandedByDefault);
  const {
    team: roles,
    roles: definitions,
    contactPoint,
  } = useChannelRoles(thread?.project || project, thread?.channel || channel);
  const team = thread ? threadTeam(thread, roles) : roles;
  const scope = thread?.channel || channel || "product";
  return (
    <div className="thread-people">
      <button
        className="team-toggle"
        aria-expanded={expanded}
        onClick={() => setExpanded(!expanded)}
        aria-label={
          thread
            ? `Participants in ${thread.title}`
            : `Channel roles for ${scope}`
        }
      >
        <span className="crew-avatars">
          {team.map((m) => (
            <Avatar name={m.name} small key={`${m.name}:${m.role}`} />
          ))}
        </span>
        <span>
          {thread
            ? `${team.length} participant${team.length === 1 ? "" : "s"}`
            : `${definitions.length} roles · ${team.length} agents`}{" "}
          <span className="team-hint">
            · {team.map((m) => m.role).join(" / ")}
          </span>
        </span>
        <Icon name={expanded ? "down" : "right"} size={12} />
      </button>
      {!thread && contactPoint && (
        <span className="channel-contact-label">
          Contact point · {displayName(contactPoint)}
        </span>
      )}
      {expanded && (
        <div className="team-roster">
          {!team.length && <p>No channel roles assigned.</p>}
          {team.map((m) => (
            <div className="team-member" key={`${m.name}:${m.role}`}>
              <Avatar name={m.name} small />
              <div>
                <strong>
                  {displayName(m.name)}
                  <span className="role-label">{m.role}</span>
                </strong>
                <p>{m.scope}</p>
              </div>
            </div>
          ))}
          {!thread &&
            definitions
              .filter((r) => !r.holders.length)
              .map((r) => (
                <div className="team-member" key={r.id}>
                  <Icon name="agents" />
                  <div>
                    <strong>
                      {r.role}
                      <span className="role-label">Unassigned</span>
                    </strong>
                    <p>{r.scope}</p>
                  </div>
                </div>
              ))}
          <p className="team-note">
            Roles inherited from #{scope}. Thread ownership does not change a
            channel role.
          </p>
        </div>
      )}
    </div>
  );
}
