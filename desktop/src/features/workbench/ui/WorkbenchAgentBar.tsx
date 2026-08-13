import { UserAvatar } from "@/shared/ui/UserAvatar";
import { cn } from "@/shared/lib/cn";
import { normalizePubkey } from "@/shared/lib/pubkey";
import type { ThreadAgentRef } from "../lib/workbenchComposerTarget";
import type { WorkbenchAgentStatus } from "../lib/workbenchThreadIndex";

export function WorkbenchAgentBar({
  agents,
  officeView,
  statusByPubkey,
  targetPubkey,
}: {
  agents: readonly ThreadAgentRef[];
  officeView: boolean;
  statusByPubkey: ReadonlyMap<string, WorkbenchAgentStatus>;
  targetPubkey: string | null;
}) {
  if (agents.length === 0) return null;
  return (
    <div
      className="flex flex-wrap gap-2 border-b border-border/60 px-4 py-2"
      data-testid="workbench-agent-bar"
    >
      {agents.map((agent) => {
        const status =
          statusByPubkey.get(normalizePubkey(agent.pubkey)) ?? "idle";
        const targeted =
          normalizePubkey(agent.pubkey) ===
          (targetPubkey ? normalizePubkey(targetPubkey) : "");
        return (
          <span
            className={cn(
              "inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs",
              targeted
                ? "border-primary/50 bg-primary/10"
                : "border-border/60 bg-muted/40",
            )}
            data-testid={`workbench-agent-chip-${agent.pubkey}`}
            key={agent.pubkey}
          >
            <UserAvatar avatarUrl={null} displayName={agent.name} size="xs" />
            <span className="font-medium">{agent.name}</span>
            {!officeView ? (
              <span className="text-2xs text-muted-foreground">{status}</span>
            ) : null}
          </span>
        );
      })}
    </div>
  );
}
