import * as React from "react";
import { toast } from "sonner";

import { useActiveTurnsByConversation } from "@/features/agents/activeAgentTurnsStore";
import {
  invalidateProjectWorktreeDetails,
  useProjectWorktreeDetailsMap,
} from "@/features/agents/projectWorktreeDetailsStore";
import {
  invalidateProjectWorktreeRegistry,
  useProjectWorktreeRegistry,
} from "@/features/agents/projectWorktreeRegistryStore";
import {
  bucketWorktrees,
  countManagedWorktrees,
  githubAvailabilityNotice,
  pruneSelectedWorktreePaths,
} from "@/features/channels/lib/worktreeBuckets";
import { formatDiskBytes } from "@/features/channels/lib/worktreeDiskFormat";
import { ChannelWorktreesDrawerBuckets } from "@/features/channels/ui/ChannelWorktreesDrawerBuckets";
import { ChannelWorktreesDrawerShell } from "@/features/channels/ui/ChannelWorktreesDrawerShell";
import { ChannelWorktreesRemoveDialog } from "@/features/channels/ui/ChannelWorktreesRemoveDialog";
import {
  evictProjectWorktree,
  pruneProjectWorktrees,
} from "@/shared/api/agentControl";
import { useTheme } from "@/shared/theme/ThemeProvider";

type ChannelWorktreesDrawerProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  repositoryPath: string | null;
  channelId?: string | null;
  channelRootIds: ReadonlySet<string>;
  rootBodiesById: ReadonlyMap<string, string>;
  onOpenThread: (rootEventId: string) => void;
};

export function ChannelWorktreesDrawer({
  open,
  onOpenChange,
  repositoryPath,
  channelId = null,
  channelRootIds,
  rootBodiesById,
  onOpenThread,
}: ChannelWorktreesDrawerProps) {
  const { isDark } = useTheme();
  const { snapshot, refresh } = useProjectWorktreeRegistry(repositoryPath);
  const detailsMap = useProjectWorktreeDetailsMap(repositoryPath);
  const activeTurns = useActiveTurnsByConversation();
  const [selected, setSelected] = React.useState<Set<string>>(new Set());
  const [confirmPaths, setConfirmPaths] = React.useState<string[] | null>(null);
  const [busy, setBusy] = React.useState(false);

  const activeRootIds = React.useMemo(
    () => new Set(activeTurns.map((turn) => turn.conversationId)),
    [activeTurns],
  );

  // Demand-driven details only (row expansion). Do not prefetch every managed
  // worktree on drawer open — that was an unbounded fanout.

  React.useEffect(() => {
    if (!open) setSelected(new Set());
  }, [open]);

  const buckets = React.useMemo(() => {
    if (snapshot.status !== "ready") return [];
    return bucketWorktrees({
      entries: snapshot.value.entries,
      channelRootIds,
      activeRootIds,
      channelId,
      detailsByPath: detailsMap,
    });
  }, [snapshot, channelRootIds, activeRootIds, channelId, detailsMap]);

  React.useEffect(() => {
    setSelected((prev) =>
      pruneSelectedWorktreePaths(prev, buckets, { activeRootIds }),
    );
  }, [buckets, activeRootIds]);

  const managedCount =
    snapshot.status === "ready"
      ? countManagedWorktrees(snapshot.value.entries)
      : 0;
  let diskTotal = 0;
  let diskKnown = 0;
  for (const details of detailsMap.values()) {
    diskTotal += details.diskBytes;
    diskKnown += 1;
  }
  const subtitle = `${managedCount} managed${
    diskKnown > 0 ? ` · ${formatDiskBytes(diskTotal)} across ${diskKnown}` : ""
  }`;

  const runRemove = async (paths: string[]) => {
    if (!repositoryPath || !channelId) return;
    setBusy(true);
    let removed = 0;
    let refused = 0;
    try {
      for (const path of paths) {
        try {
          const result = await evictProjectWorktree(
            repositoryPath,
            path,
            channelId,
          );
          if (result.status === "completed") removed += 1;
          else refused += 1;
        } catch {
          refused += 1;
        }
      }
      toast.message(`${removed} freed · ${refused} refused`);
      invalidateProjectWorktreeDetails(repositoryPath);
      invalidateProjectWorktreeRegistry(repositoryPath);
      await refresh();
    } finally {
      setBusy(false);
      setConfirmPaths(null);
      setSelected(new Set());
    }
  };

  const runPrune = async () => {
    if (!repositoryPath) return;
    setBusy(true);
    try {
      const result = await pruneProjectWorktrees(repositoryPath);
      result.status === "completed"
        ? toast.success(result.message)
        : toast.error(result.message);
      invalidateProjectWorktreeRegistry(repositoryPath);
      await refresh();
    } catch {
      toast.error("Prune failed.");
    } finally {
      setBusy(false);
    }
  };

  if (!repositoryPath) return null;

  const githubNotice =
    snapshot.status === "ready"
      ? githubAvailabilityNotice(snapshot.value.github)
      : null;

  return (
    <>
      <ChannelWorktreesDrawerShell
        busy={busy}
        isDark={isDark}
        onOpenChange={onOpenChange}
        onRemoveSelected={() => setConfirmPaths([...selected])}
        open={open}
        selectedCount={selected.size}
        subtitle={subtitle}
      >
        {snapshot.status === "pending" ? (
          <p className="text-sm text-muted-foreground">Loading…</p>
        ) : null}
        {snapshot.status === "error" ? (
          <p className="text-sm text-destructive">{snapshot.message}</p>
        ) : null}
        {githubNotice ? (
          <p
            className="mb-2 text-sm text-muted-foreground"
            data-testid="channel-worktrees-github-notice"
          >
            {githubNotice}
          </p>
        ) : null}
        <ChannelWorktreesDrawerBuckets
          activeRootIds={activeRootIds}
          buckets={buckets}
          channelId={channelId}
          onCacheCleared={() => {
            invalidateProjectWorktreeDetails(repositoryPath);
            invalidateProjectWorktreeRegistry(repositoryPath);
            void refresh();
          }}
          onOpenThread={onOpenThread}
          onPrune={() => void runPrune()}
          onRemove={(path) => setConfirmPaths([path])}
          onToggleSelect={(path) => {
            setSelected((prev) => {
              const next = new Set(prev);
              if (next.has(path)) next.delete(path);
              else next.add(path);
              return next;
            });
          }}
          repositoryPath={repositoryPath}
          rootBodiesById={rootBodiesById}
          selected={selected}
        />
      </ChannelWorktreesDrawerShell>

      <ChannelWorktreesRemoveDialog
        busy={busy}
        onCancel={() => setConfirmPaths(null)}
        onConfirm={(paths) => void runRemove(paths)}
        paths={confirmPaths}
      />
    </>
  );
}
