/**
 * Crew role taxonomy + UI helpers (issue #116 Slice 1).
 * Single source of truth for the day-one list mirrored in Rust
 * `managed_agents::crew_role::TAXONOMY`.
 */

export const CREW_ROLE_TAXONOMY = [
  "code",
  "content",
  "research",
  "ops",
] as const;

export type CrewRole = (typeof CREW_ROLE_TAXONOMY)[number];

export function isCrewRole(
  value: string | null | undefined,
): value is CrewRole {
  return (
    typeof value === "string" &&
    (CREW_ROLE_TAXONOMY as readonly string[]).includes(value)
  );
}

export function crewRoleLabel(role: string | null | undefined): string {
  if (!role) return "No role";
  return role;
}

export function crewRoleSubmitPatch(
  draft: string,
  saved: string | null | undefined,
): string | null | undefined {
  const next = draft.trim() || null;
  const prev = saved ?? null;
  return next !== prev ? next : undefined;
}
