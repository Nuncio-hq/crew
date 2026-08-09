import type { RelaySubscriptionFilter } from "@/shared/api/relayClientShared";

/** Exact ids authorize a producer-kind-independent relationship lookup. */
export function buildReceiptParentFilter(
  ids: string[],
): RelaySubscriptionFilter {
  return { ids, limit: ids.length };
}
