import type { RelaySubscriptionFilter } from "@/shared/api/relayClientShared";
import type { RelayEvent } from "@/shared/api/types";
import {
  KIND_AGENT_RECEIPT,
  KIND_AGENT_USER_INPUT_ANSWER,
  KIND_AGENT_USER_INPUT_REQUESTED,
  KIND_AGENT_USER_INPUT_RESOLVED,
  KIND_REACTION,
} from "@/shared/constants/kinds";

type FetchPage = (filter: RelaySubscriptionFilter) => Promise<RelayEvent[]>;

function byDurableOrder(left: RelayEvent, right: RelayEvent): number {
  return left.created_at - right.created_at || left.id.localeCompare(right.id);
}

function byUserInputAuthorityOrder(
  left: RelayEvent,
  right: RelayEvent,
): number {
  const leftPriority = left.kind === KIND_AGENT_USER_INPUT_REQUESTED ? 0 : 1;
  const rightPriority = right.kind === KIND_AGENT_USER_INPUT_REQUESTED ? 0 : 1;
  return leftPriority - rightPriority || byDurableOrder(left, right);
}

/** Merge a completed history snapshot with events buffered by its live overlap. */
export function mergeDurableActionEvents(
  userInputEvents: readonly RelayEvent[],
  receiptEvents: readonly RelayEvent[],
  reviewEvents: readonly RelayEvent[],
  bufferedEvents: readonly RelayEvent[],
) {
  const byId = new Map<string, RelayEvent>();
  for (const event of [
    ...userInputEvents,
    ...receiptEvents,
    ...reviewEvents,
    ...bufferedEvents,
  ]) {
    byId.set(event.id, event);
  }
  const merged = [...byId.values()];
  return {
    userInputEvents: merged
      .filter((event) =>
        [
          KIND_AGENT_USER_INPUT_REQUESTED,
          KIND_AGENT_USER_INPUT_ANSWER,
          KIND_AGENT_USER_INPUT_RESOLVED,
        ].includes(event.kind),
      )
      .sort(byUserInputAuthorityOrder),
    receiptEvents: merged
      .filter((event) => event.kind === KIND_AGENT_RECEIPT)
      .sort(byDurableOrder),
    reviewEvents: merged
      .filter((event) => event.kind === KIND_REACTION)
      .sort(byDurableOrder),
  };
}

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
