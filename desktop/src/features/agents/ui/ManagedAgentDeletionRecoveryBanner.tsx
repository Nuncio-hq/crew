import { useCommunities } from "@/features/communities/useCommunities";
import {
  useManagedAgentDeletionsQuery,
  useRetryManagedAgentDeletionMutation,
} from "@/features/agents/managedAgentDeletionHooks";
import { Button } from "@/shared/ui/button";

function communityOrigin(value: string): string {
  try {
    const url = new URL(value);
    const protocol =
      url.protocol === "ws:"
        ? "http:"
        : url.protocol === "wss:"
          ? "https:"
          : url.protocol;
    return `${protocol}//${url.host}`.toLowerCase();
  } catch {
    return value.replace(/\/+$/, "").toLowerCase();
  }
}

export function ManagedAgentDeletionRecoveryBanner() {
  const { communities, activeCommunity, switchCommunity } = useCommunities();
  const query = useManagedAgentDeletionsQuery();
  const retry = useRetryManagedAgentDeletionMutation();

  if (!query.data?.length) {
    return null;
  }

  const activeOrigin = activeCommunity
    ? communityOrigin(activeCommunity.relayUrl)
    : null;

  return (
    <section
      aria-label="Managed-agent deletion recovery"
      className="rounded-xl border border-amber-400/40 bg-amber-50/70 px-4 py-3 text-sm text-amber-950 dark:bg-amber-950/20 dark:text-amber-100"
      data-testid="managed-agent-deletion-recovery"
    >
      <p className="font-medium">Agent cleanup needs attention</p>
      <p className="mt-1 text-xs opacity-80">
        A pending deletion keeps its agent stopped until cleanup is retried in
        the community that recorded it.
      </p>
      <ul className="mt-3 space-y-2">
        {query.data.map((operation) => {
          const target = communities.find(
            (community) =>
              communityOrigin(community.relayUrl) ===
              communityOrigin(operation.community),
          );
          const isActive =
            activeOrigin === communityOrigin(operation.community);
          return (
            <li
              className="flex flex-wrap items-center justify-between gap-2"
              key={`${operation.id}:${operation.community}`}
            >
              <span className="min-w-0 break-all text-xs">
                {operation.resourceKey.slice(0, 12)}… · {operation.community}
              </span>
              {target && !isActive ? (
                <Button
                  onClick={() => switchCommunity(target.id)}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  Switch community
                </Button>
              ) : isActive ? (
                <Button
                  disabled={retry.isPending}
                  onClick={() => retry.mutate(operation.id)}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  {retry.isPending ? "Retrying…" : "Retry cleanup"}
                </Button>
              ) : (
                <span className="text-xs opacity-80">
                  Reconnect this community to retry
                </span>
              )}
            </li>
          );
        })}
      </ul>
      {retry.error ? (
        <p className="mt-2 text-xs" role="alert">
          {retry.error instanceof Error
            ? retry.error.message
            : "Cleanup retry failed."}
        </p>
      ) : null}
    </section>
  );
}
