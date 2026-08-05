import type { WorktreeBucket } from "@/features/channels/lib/worktreeBuckets";
import { ChannelWorktreeRow } from "@/features/channels/ui/ChannelWorktreeRow";

type ChannelWorktreesDrawerBucketsProps = {
  buckets: WorktreeBucket[];
  repositoryPath: string;
  channelId?: string | null;
  rootBodiesById: ReadonlyMap<string, string>;
  selected: ReadonlySet<string>;
  activeRootIds: ReadonlySet<string>;
  onToggleSelect: (path: string) => void;
  onOpenThread?: (rootEventId: string) => void;
  onRemove: (path: string) => void;
  onPrune: () => void;
  onCacheCleared?: () => void;
};

export function ChannelWorktreesDrawerBuckets({
  buckets,
  repositoryPath,
  channelId = null,
  rootBodiesById,
  selected,
  activeRootIds,
  onToggleSelect,
  onOpenThread,
  onRemove,
  onPrune,
  onCacheCleared,
}: ChannelWorktreesDrawerBucketsProps) {
  return (
    <>
      {buckets.map((bucket) => (
        <section key={bucket.id} className="space-y-2">
          <div className="flex items-baseline gap-2 px-1">
            <h3 className="text-2xs font-semibold uppercase tracking-wide text-muted-foreground">
              {bucket.label}{" "}
              <span className="font-normal">{bucket.items.length}</span>
            </h3>
            <span className="text-3xs text-muted-foreground/80">
              {bucket.hint}
            </span>
          </div>
          <div className="space-y-2">
            {bucket.items.map((item) => (
              <ChannelWorktreeRow
                key={item.entry.worktreePath}
                activeRootIds={activeRootIds}
                channelId={channelId}
                item={item}
                onOpenThread={onOpenThread}
                onPrune={bucket.id === "broken" ? () => onPrune() : undefined}
                onCacheCleared={onCacheCleared}
                onRemove={onRemove}
                onToggleSelect={onToggleSelect}
                readonly={bucket.readonly}
                repositoryPath={repositoryPath}
                rootBody={
                  item.entry.rootEventId
                    ? (rootBodiesById.get(
                        item.entry.rootEventId.toLowerCase(),
                      ) ?? null)
                    : null
                }
                selected={selected.has(item.entry.worktreePath)}
              />
            ))}
          </div>
        </section>
      ))}
    </>
  );
}
