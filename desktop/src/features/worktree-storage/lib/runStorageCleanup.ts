import {
  clearProjectWorktreeCache,
  evictProjectWorktree,
  revalidateWorktreeStorageAction,
} from "@/shared/api/agentControl";
import type {
  ReclaimTier,
  WorktreeStorageRow,
  WorktreeStorageRowOutcome,
} from "@/shared/api/thread-workspace-types";

export type StorageCleanupProgress = {
  worktreePath: string;
  outcome: WorktreeStorageRowOutcome;
};

/**
 * Sequential suggest-and-confirm runner over existing #72 commands.
 * Mid-run refusals are outcomes (skip + continue), never hard errors.
 */
export async function runStorageCleanup(input: {
  rows: WorktreeStorageRow[];
  onProgress: (progress: StorageCleanupProgress) => void;
}): Promise<void> {
  for (const row of input.rows) {
    input.onProgress({
      worktreePath: row.worktreePath,
      outcome: { status: "running" },
    });
    const outcome = await runOne(row);
    input.onProgress({ worktreePath: row.worktreePath, outcome });
  }
}

async function runOne(
  row: WorktreeStorageRow,
): Promise<
  Exclude<WorktreeStorageRowOutcome, { status: "pending" | "running" }>
> {
  const channelId = row.routingChannelId?.trim();
  if (!channelId) {
    return {
      status: "skipped",
      message: "skipped: missing channel identity",
    };
  }
  const tier: ReclaimTier = row.tier ?? "lean";
  try {
    const refusal = await revalidateWorktreeStorageAction(
      row.repositoryPath,
      row.worktreePath,
      channelId,
      tier,
    );
    if (refusal) {
      return { status: "skipped", message: `skipped: ${refusal}` };
    }
    if (tier === "hibernate") {
      const result = await evictProjectWorktree(
        row.repositoryPath,
        row.worktreePath,
        channelId,
      );
      if (result.status === "completed") {
        return {
          status: "completed",
          message: `freed ${formatBytesShort(row.diskBytes)}`,
          bytesFreed: row.diskBytes,
        };
      }
      return {
        status: "skipped",
        message: `skipped: ${result.message}`,
      };
    }

    const result = await clearProjectWorktreeCache(
      row.repositoryPath,
      row.worktreePath,
      row.cacheCategoryIds,
      channelId,
    );
    const bytesFreed = result.results.reduce(
      (sum, item) => sum + (item.bytesRemoved || 0),
      0,
    );
    const refused = result.results.find((item) => item.status === "refused");
    if (refused && bytesFreed === 0) {
      return {
        status: "skipped",
        message: `skipped: ${refused.message}`,
      };
    }
    return {
      status: "completed",
      message: `freed ${formatBytesShort(bytesFreed || row.cacheBytes)}`,
      bytesFreed: bytesFreed || row.cacheBytes,
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : "cleanup failed";
    return { status: "skipped", message: `skipped: ${message}` };
  }
}

function formatBytesShort(bytes: number): string {
  const gb = bytes / (1024 * 1024 * 1024);
  if (gb >= 1) return `${gb.toFixed(gb >= 10 ? 0 : 1)} GB`;
  const mb = bytes / (1024 * 1024);
  if (mb >= 1) return `${Math.round(mb)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}
