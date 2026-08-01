import { FolderGit2, Users } from "lucide-react";

import type {
  ProjectThreadAgentStep,
  ProjectThreadContext,
} from "@/features/messages/lib/projectThreadWorkspace";
import { truncatePubkey } from "@/shared/lib/pubkey";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { UserAvatar } from "@/shared/ui/UserAvatar";

export function ProjectThreadTaskDetails({
  context,
  profiles,
  steps,
}: {
  context: ProjectThreadContext;
  profiles?: UserProfileLookup;
  steps: readonly ProjectThreadAgentStep[];
}) {
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <Users className="h-4 w-4 text-muted-foreground" />
        <div>
          <p className="text-sm font-semibold">{steps.length}-agent task</p>
          <p className="text-xs text-muted-foreground">
            One feature branch shared by every agent in this thread.
          </p>
        </div>
      </div>
      <div className="flex flex-wrap gap-2">
        {steps.map((step) => {
          const profile = profiles?.[step.pubkey];
          const name =
            profile?.displayName ??
            profile?.name ??
            truncatePubkey(step.pubkey);
          return (
            <div
              className="flex items-center gap-2 rounded-lg border border-border/60 px-2 py-1.5"
              key={step.pubkey}
            >
              <UserAvatar
                avatarUrl={profile?.avatarUrl ?? null}
                displayName={name}
                size="sm"
              />
              <span className="text-xs font-medium">{name}</span>
            </div>
          );
        })}
      </div>
      <div className="flex gap-2 rounded-lg bg-muted/50 p-2">
        <FolderGit2 className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0 text-xs">
          <p className="truncate font-medium">{context.repoAddress}</p>
          <p className="truncate text-muted-foreground">{context.localPath}</p>
        </div>
      </div>
    </div>
  );
}
