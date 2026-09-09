// Prototype-only routing. Production must use verified author identities and
// structured mention targets, not this plain-text display-name matcher.
export function sampleRecipients({
  text,
  agents,
  project,
  channel,
  contactPoint,
  authorKind = "human",
}) {
  if (authorKind !== "human") return { recipients: [], unavailable: [] };
  const members = agents.filter((a) =>
    a.channels.includes(`${project}/${channel}`),
  );
  const hasMention = /(^|\s)@\S/u.test(text);
  const escaped = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const targets = hasMention
    ? members.filter((a) =>
        new RegExp(`(^|\\s)@${escaped(a.name)}(?=$|[\\s.,!?:;])`, "iu").test(
          text,
        ),
      )
    : members.filter((a) => a.id === contactPoint);
  const ready = (a) => ["Working", "Available"].includes(a.status);
  return {
    recipients: targets.filter(ready).map((a) => a.id),
    unavailable: targets.filter((a) => !ready(a)).map((a) => a.name),
    missingContact:
      !hasMention && Boolean(contactPoint) && targets.length === 0,
  };
}
