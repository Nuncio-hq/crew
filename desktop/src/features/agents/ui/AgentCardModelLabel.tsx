import { resolveAgentCardModelLabel } from "@/features/agents/lib/agentCardModelLabel";
import { useHermesProfileModelDisplay } from "@/features/agents/hooks/useHermesProfileModelDisplay";
import type { ManagedAgent } from "@/shared/api/types";

export function useAgentCardModelLabel({
  agent,
  personaModel,
  defaultModel,
}: {
  agent: Pick<ManagedAgent, "modelSource" | "model" | "hermesProfile"> | undefined;
  personaModel: string | null | undefined;
  defaultModel: string;
}): string {
  const profileModel = useHermesProfileModelDisplay(agent?.hermesProfile);
  return resolveAgentCardModelLabel({
    agent,
    personaModel,
    defaultModel,
    profileModelFromDisk: profileModel,
  });
}
