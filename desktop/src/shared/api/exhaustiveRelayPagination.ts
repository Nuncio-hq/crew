import type { RelaySubscriptionFilter } from "@/shared/api/relayClientShared";
import type { RelayEvent } from "@/shared/api/types";

const HEX_PREFIX_DIGITS = "0123456789abcdef";

type FetchPage = (filter: RelaySubscriptionFilter) => Promise<RelayEvent[]>;
type BaseFilter = Omit<RelaySubscriptionFilter, "limit" | "since" | "until">;

async function drainTimestampPrefix(
  fetchPage: FetchPage,
  baseFilter: BaseFilter,
  timestamp: number,
  pageSize: number,
  prefix: string,
): Promise<RelayEvent[]> {
  const page = await fetchPage({
    ...baseFilter,
    ids: [prefix],
    limit: pageSize,
    since: timestamp,
    until: timestamp,
  });
  if (page.length < pageSize || prefix.length >= 64) return page;

  const events: RelayEvent[] = [];
  for (const digit of HEX_PREFIX_DIGITS) {
    events.push(
      ...(await drainTimestampPrefix(
        fetchPage,
        baseFilter,
        timestamp,
        pageSize,
        `${prefix}${digit}`,
      )),
    );
  }
  return events;
}

/** Exhaustively drain one Nostr timestamp bucket using event-id prefixes. */
export async function drainRelayTimestampBucket(
  fetchPage: FetchPage,
  baseFilter: BaseFilter,
  timestamp: number,
  pageSize: number,
): Promise<RelayEvent[]> {
  if (!Number.isInteger(pageSize) || pageSize < 1) {
    throw new Error(
      "relay timestamp bucket page size must be a positive integer",
    );
  }
  const prefixes =
    baseFilter.ids && baseFilter.ids.length > 0
      ? baseFilter.ids
      : [...HEX_PREFIX_DIGITS];
  const events: RelayEvent[] = [];
  for (const prefix of prefixes) {
    events.push(
      ...(await drainTimestampPrefix(
        fetchPage,
        baseFilter,
        timestamp,
        pageSize,
        prefix,
      )),
    );
  }
  return events;
}

/** Exhaustively enumerate relay history without skipping equal-timestamp events. */
export async function fetchExhaustiveRelayHistory(
  fetchPage: FetchPage,
  baseFilter: BaseFilter,
  pageSize: number,
): Promise<RelayEvent[]> {
  if (!Number.isSafeInteger(pageSize) || pageSize < 1) {
    throw new Error("relay history page size must be a positive integer");
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
    const boundary = await drainRelayTimestampBucket(
      fetchPage,
      baseFilter,
      oldest,
      pageSize,
    );
    for (const event of boundary) byId.set(event.id, event);
    if (oldest <= 0) return [...byId.values()];
    until = oldest - 1;
  }
}
