import { GitBranch } from "lucide-react";

import { useProjectWorktreeRegistry } from "@/features/agents/projectWorktreeRegistryStore";
import {
  countManagedWorktrees,
  countOpenPullRequests,
} from "@/features/channels/lib/worktreeBuckets";
import { Button } from "@/shared/ui/button";

type ChannelWorktreesPillProps = {
  repositoryPath: string | null;
  onOpen: () => void;
};

export function ChannelWorktreesPill({
  repositoryPath,
  onOpen,
}: ChannelWorktreesPillProps) {
  const { snapshot } = useProjectWorktreeRegistry(repositoryPath);
  if (!repositoryPath || snapshot.status !== "ready") return null;

  const managed = countManagedWorktrees(snapshot.value.entries);
  if (managed === 0) return null;
  const openPrs = countOpenPullRequests(snapshot.value.entries);
  const label =
    openPrs > 0
      ? `${managed} worktrees · ${openPrs} PR${openPrs === 1 ? "" : "s"} open`
      : `${managed} worktrees`;

  return (
    <Button
      className="h-6 gap-1 px-2 text-2xs font-medium text-muted-foreground"
      data-testid="channel-worktrees-pill"
      onClick={onOpen}
      size="sm"
      title="Manage project worktrees"
      type="button"
      variant="ghost"
    >
      <GitBranch className="h-3 w-3" />
      <span>{label}</span>
    </Button>
  );
}
