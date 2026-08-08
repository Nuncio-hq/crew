import type { RelaySubscriptionFilter } from "@/shared/api/relayClientShared";
import type { RelayEvent } from "@/shared/api/types";

type FetchPage = (filter: RelaySubscriptionFilter) => Promise<RelayEvent[]>;

/** Exhaustively enumerate immutable durable events without skipping timestamp ties. */
export async function enumerateDurableActionEvents(
  fetchPage: FetchPage,
  baseFilter: Omit<RelaySubscriptionFilter, "limit" | "since" | "until">,
  pageSize: number,
): Promise<RelayEvent[]> {
  if (!Number.isSafeInteger(pageSize) || pageSize <= 0) {
    throw new Error("Durable action page size must be a positive integer.");
  }

  const byId = new Map<string, RelayEvent>();
  let until: number | undefined;
  for (;;) {
    const page = await fetchPage({
      ...baseFilter,
      limit: pageSize,
      ...(until === undefined ? {} : { until }),
    });
    for (const event of page) byId.set(event.id, event);
    if (page.length < pageSize) return [...byId.values()];

    const oldest = Math.min(...page.map((event) => event.created_at));
    const boundary = await fetchPage({
      ...baseFilter,
      limit: pageSize,
      since: oldest,
      until: oldest,
    });
    for (const event of boundary) byId.set(event.id, event);
    if (boundary.length >= pageSize) {
      throw new Error(
        "Durable action hydration cannot drain a relay timestamp bucket at the configured page limit.",
      );
    }
    if (oldest <= 0) return [...byId.values()];
    until = oldest - 1;
  }
}
