import * as React from "react";
import { toast } from "sonner";

import {
  isManagedAgentActive,
  respawnManagedAgentWithRules,
  startManagedAgentWithRules,
  stopManagedAgentWithRules,
} from "@/features/agents/lib/managedAgentControlActions";
import { clearActiveTurnsForAgentOnStop } from "@/features/agents/managedAgentRuntimeHooks";
import {
  useAgentControlScope,
  type AgentControlNativeScope,
} from "@/features/agents/lib/useAgentControlScope";
import type { Channel, ManagedAgent, RelayAgent } from "@/shared/api/types";

export function useAgentLifecycleActions({
  channels,
  managedAgent,
  relayAgents,
  startManagedAgent,
  stopManagedAgent,
}: {
  channels: readonly Channel[] | undefined;
  managedAgent: ManagedAgent | undefined;
  relayAgents: readonly RelayAgent[] | undefined;
  startManagedAgent: (
    input: string | ({ pubkey: string } & AgentControlNativeScope),
  ) => Promise<unknown>;
  stopManagedAgent: (pubkey: string) => Promise<unknown>;
}) {
  const captureControl = useAgentControlScope(managedAgent?.pubkey ?? null);
  const handleAgentPrimaryAction = React.useCallback(async () => {
    if (!managedAgent) return;
    const control = captureControl();

    try {
      if (isManagedAgentActive(managedAgent)) {
        const result = await stopManagedAgentWithRules({
          agent: managedAgent,
          channels: channels ?? [],
          relayAgents: relayAgents ?? [],
          stopManagedAgent,
        });
        if (!control.isCurrent()) return;
        if (managedAgent.backend.type === "local") {
          clearActiveTurnsForAgentOnStop(managedAgent.pubkey);
        }
        toast.success(result.noticeMessage ?? `Stopped ${managedAgent.name}.`);
        return;
      }

      await startManagedAgentWithRules({
        agent: managedAgent,
        startManagedAgent: (pubkey) =>
          startManagedAgent({ pubkey, ...control.nativeScope() }),
      });
      if (!control.isCurrent()) return;
      toast.success(
        managedAgent.backend.type === "provider"
          ? `Deploying ${managedAgent.name}.`
          : `Started ${managedAgent.name}.`,
      );
    } catch (error) {
      if (!control.isCurrent()) return;
      toast.error(
        error instanceof Error ? error.message : "Agent action failed.",
      );
    }
  }, [
    channels,
    captureControl,
    managedAgent,
    relayAgents,
    startManagedAgent,
    stopManagedAgent,
  ]);

  const handleAgentRestart = React.useCallback(async () => {
    if (!managedAgent) return;
    const control = captureControl();

    try {
      control.nativeScope();
      await respawnManagedAgentWithRules({
        agent: managedAgent,
        startManagedAgent: (pubkey) =>
          startManagedAgent({ pubkey, ...control.nativeScope() }),
        stopManagedAgent,
        onStopped: () => {
          if (control.isCurrent())
            clearActiveTurnsForAgentOnStop(managedAgent.pubkey);
        },
      });
      if (!control.isCurrent()) return;
      toast.success(`Restarted ${managedAgent.name}.`);
    } catch (error) {
      if (!control.isCurrent()) return;
      toast.error(
        error instanceof Error ? error.message : "Agent restart failed.",
      );
    }
  }, [captureControl, managedAgent, startManagedAgent, stopManagedAgent]);

  return { handleAgentPrimaryAction, handleAgentRestart };
}
