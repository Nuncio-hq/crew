import * as React from "react";

import type { ManagedAgent, RelayAgent } from "@/shared/api/types";
import { normalizePubkey } from "@/shared/lib/pubkey";
import { Badge } from "@/shared/ui/badge";
import { Button } from "@/shared/ui/button";
import { getPresenceLabel } from "@/features/presence/lib/presence";
import type { AgentAvailabilityReader } from "../lib/useAgentAvailability";
import { AgentIdentityCard } from "./AgentIdentityCard";
import { AgentManagementMarker } from "./OtherSetupAgentMarker";

/** Read-only relay projection; local custody and runtime controls stay elsewhere. */
export function RelayOnlyAgentsSection({
  agents,
  relayAgents,
  localInventoryReady,
  relayAgentsReady,
  relayAgentsError,
  isAgentsLoading,
  isRelayAgentsLoading,
  isArchived,
  gridClassName,
  getAvailability,
  onOpenAgentProfile,
  onRetryAgents,
  onRetryRelayAgents,
}: {
  agents: readonly ManagedAgent[];
  relayAgents: readonly RelayAgent[];
  localInventoryReady: boolean;
  relayAgentsReady: boolean;
  relayAgentsError: Error | null;
  isAgentsLoading: boolean;
  isRelayAgentsLoading: boolean;
  isArchived: (pubkey: string) => boolean;
  gridClassName: string;
  getAvailability: AgentAvailabilityReader;
  onOpenAgentProfile: (pubkey: string) => void;
  onRetryAgents: () => void;
  onRetryRelayAgents: () => void;
}) {
  const additional = React.useMemo(() => {
    if (!localInventoryReady || !relayAgentsReady) return [];
    const seen = new Set(agents.map((agent) => normalizePubkey(agent.pubkey)));
    return relayAgents.filter((agent) => {
      const key = normalizePubkey(agent.pubkey);
      if (!key || seen.has(key) || isArchived(key)) return false;
      seen.add(key);
      return true;
    });
  }, [agents, relayAgents, localInventoryReady, relayAgentsReady, isArchived]);

  return (
    <section className="min-w-0 space-y-3" data-testid="relay-only-agents">
      <div>
        <h3 className="text-sm font-medium">Relay agents</h3>
        <p className="text-sm text-muted-foreground">
          Visible in this community. Open a profile to see available actions.
        </p>
      </div>
      {!localInventoryReady ? (
        <div className="space-y-2 text-sm text-muted-foreground">
          <p>
            {isAgentsLoading
              ? "Checking local management inventory…"
              : "Local management inventory is unavailable. Relay-only classification is unknown."}
          </p>
          {!isAgentsLoading ? (
            <Button onClick={onRetryAgents} size="sm" variant="outline">
              Retry local inventory
            </Button>
          ) : null}
        </div>
      ) : !relayAgentsReady ? (
        <div
          className="space-y-2 text-sm text-muted-foreground"
          role={relayAgentsError ? "alert" : "status"}
        >
          <p className="break-words">
            {relayAgentsError?.message ??
              (isRelayAgentsLoading
                ? "Loading relay agents…"
                : "Relay agents are unavailable.")}
          </p>
          {!isRelayAgentsLoading ? (
            <Button onClick={onRetryRelayAgents} size="sm" variant="outline">
              Retry relay agents
            </Button>
          ) : null}
        </div>
      ) : additional.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No additional relay agents.
        </p>
      ) : (
        <div className={gridClassName}>
          {additional.map((agent) => {
            const key = normalizePubkey(agent.pubkey);
            const availability = getAvailability(key);
            return (
              <AgentIdentityCard
                key={key}
                ariaLabel={`${agent.name} agent profile`}
                dataTestId={`relay-only-agent-${key}`}
                label={agent.name}
                subtitle="Relay-visible agent"
                footerAccessory={
                  <AgentManagementMarker
                    pubkey={key}
                    ownerPubkey={agent.ownerPubkey}
                  />
                }
                statusBadge={
                  <Badge variant="secondary">
                    {availability
                      ? getPresenceLabel(availability)
                      : "Availability unknown"}
                  </Badge>
                }
                onClick={() => onOpenAgentProfile(key)}
              />
            );
          })}
        </div>
      )}
    </section>
  );
}
