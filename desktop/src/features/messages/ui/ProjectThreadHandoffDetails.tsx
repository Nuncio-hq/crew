import { Check, Clock3 } from "lucide-react";

import type { ProjectThreadAgentStep } from "@/features/messages/lib/projectThreadWorkspace";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { truncatePubkey } from "@/shared/lib/pubkey";
import { UserAvatar } from "@/shared/ui/UserAvatar";

function StepStatus({ status }: { status: ProjectThreadAgentStep["status"] }) {
  if (status === "done") {
    return (
      <span className="flex items-center gap-1 text-xs text-success">
        <Check className="h-3.5 w-3.5" /> Done
      </span>
    );
  }
  if (status === "working") {
    return (
      <span className="flex items-center gap-1 text-xs text-success">
        <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-current" />
        Working
      </span>
    );
  }
  return (
    <span className="flex items-center gap-1 text-xs text-muted-foreground">
      <Clock3 className="h-3.5 w-3.5" /> Queued
    </span>
  );
}

export function ProjectThreadHandoffDetails({
  profiles,
  steps,
}: {
  profiles?: UserProfileLookup;
  steps: readonly ProjectThreadAgentStep[];
}) {
  return (
    <div className="overflow-hidden rounded-lg border border-border/60">
      {steps.map((step, index) => {
        const profile = profiles?.[step.pubkey];
        const name =
          profile?.displayName ?? profile?.name ?? truncatePubkey(step.pubkey);
        return (
          <div
            className="flex items-center gap-2.5 border-b border-border/40 px-3 py-2 last:border-b-0"
            key={step.pubkey}
          >
            <span className="w-4 text-center text-xs text-muted-foreground">
              {index + 1}
            </span>
            <UserAvatar
              avatarUrl={profile?.avatarUrl ?? null}
              displayName={name}
              size="sm"
            />
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-medium">{name}</p>
              <p className="text-2xs text-muted-foreground">
                {step.source === "root" ? "Root mention" : "Added in a reply"}
              </p>
            </div>
            <StepStatus status={step.status} />
          </div>
        );
      })}
    </div>
  );
}
