export type CrewFindingSeverity = "error" | "warning" | "info" | "note";

export type CrewFinding = {
  severity: CrewFindingSeverity;
  file: string | null;
  range: string | null;
};

const SEVERITIES = new Set<CrewFindingSeverity>([
  "error",
  "warning",
  "info",
  "note",
]);

/** Tolerant `["crew-finding", severity, file, range]` — first tag wins. */
export function parseCrewFinding(
  tags: readonly (readonly string[])[] | undefined,
): CrewFinding | null {
  const tag = tags?.find((entry) => entry[0] === "crew-finding");
  if (!tag) return null;
  const raw = (tag[1] ?? "info").trim().toLowerCase();
  const severity: CrewFindingSeverity = SEVERITIES.has(
    raw as CrewFindingSeverity,
  )
    ? (raw as CrewFindingSeverity)
    : "info";
  return {
    severity,
    file: tag[2]?.trim() ? tag[2] : null,
    range: tag[3]?.trim() ? tag[3] : null,
  };
}
