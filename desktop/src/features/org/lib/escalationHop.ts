import {
  displayNameForPubkey,
  isOfficer,
  managerOf,
  type OrgRoster,
} from "@/features/org/lib/orgRoster";

export function escalationHopLabel(
  roster: OrgRoster | null,
  agentPubkey: string,
  profiles: Record<
    string,
    { displayName?: string | null; name?: string | null }
  >,
): string | null {
  if (!roster) {
    return null;
  }
  const manager = managerOf(roster, agentPubkey);
  if (!manager) {
    return null;
  }
  const agentName = displayNameForPubkey(agentPubkey, profiles);
  const managerName = displayNameForPubkey(manager, profiles);
  return `${agentName} → ${managerName}`;
}

export function isOfficerLevelAgent(
  roster: OrgRoster,
  agentPubkey: string,
): boolean {
  return isOfficer(roster, agentPubkey);
}
