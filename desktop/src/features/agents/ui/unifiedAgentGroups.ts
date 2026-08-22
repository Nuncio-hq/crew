import { isManagedAgentActive } from "@/features/agents/lib/managedAgentControlActions";
import type { AgentPersona, ManagedAgent } from "@/shared/api/types";

type PersonaGroup = { persona: AgentPersona; agents: ManagedAgent[] };

/**
 * Whether a managed agent's identity is archived.
 *
 * Callers pass `useIsArchivedPredicate()`, which fails open (nothing archived)
 * until the archive snapshot hydrates, so a slow snapshot degrades to today's
 * behavior instead of hiding live agents.
 */
type IsArchived = (pubkey: string) => boolean;

/**
 * Archived standalone and unknown-persona agents are dropped because nothing
 * else would ever surface them again, while a matched persona group keeps its
 * archived instances so the persona card itself survives; the card resolves a
 * live target through {@link pickProfileAgent}.
 */
export function buildUnifiedGroups(
  personas: AgentPersona[],
  agents: ManagedAgent[],
  isArchived: IsArchived,
) {
  const byPersonaId = new Map<string, ManagedAgent[]>();
  const ungrouped: ManagedAgent[] = [];

  for (const agent of agents) {
    if (!agent.personaId) {
      ungrouped.push(agent);
    } else {
      const list = byPersonaId.get(agent.personaId) ?? [];
      list.push(agent);
      byPersonaId.set(agent.personaId, list);
    }
  }

  const matched = new Set<string>();
  const groups: PersonaGroup[] = personas.map((persona) => {
    matched.add(persona.id);
    return { persona, agents: byPersonaId.get(persona.id) ?? [] };
  });

  const unknown: ManagedAgent[] = [];
  for (const [id, list] of byPersonaId) {
    if (!matched.has(id)) unknown.push(...list);
  }

  return {
    groups,
    ungrouped: ungrouped.filter((agent) => !isArchived(agent.pubkey)),
    unknown: unknown.filter((agent) => !isArchived(agent.pubkey)),
  };
}

/**
 * The instance a persona-level surface acts on: the highest-ranked live
 * instance, or `undefined` when every instance is archived.
 */
export function pickProfileAgent(
  agents: readonly ManagedAgent[],
  isArchived: IsArchived,
) {
  return agents
    .filter((agent) => !isArchived(agent.pubkey))
    .sort((left, right) => {
      const activeDiff =
        Number(isManagedAgentActive(right)) -
        Number(isManagedAgentActive(left));
      if (activeDiff !== 0) return activeDiff;
      return left.name.localeCompare(right.name);
    })[0];
}
