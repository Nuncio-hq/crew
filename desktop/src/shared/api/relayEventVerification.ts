import { verifyEvent } from "nostr-tools/pure";

import type { RelaySubscriptionFilter } from "./relayClientShared";
import type { RelayEvent } from "./types";

function matchesPrefixes(value: string, prefixes: readonly string[]): boolean {
  return (
    prefixes.length > 0 && prefixes.some((prefix) => value.startsWith(prefix))
  );
}

export function relayEventMatchesFilter(
  event: RelayEvent,
  filter: RelaySubscriptionFilter,
): boolean {
  if (filter.ids && !matchesPrefixes(event.id, filter.ids)) return false;
  if (filter.authors && !matchesPrefixes(event.pubkey, filter.authors)) {
    return false;
  }
  if (filter.kinds && !filter.kinds.includes(event.kind)) return false;
  if (filter.since !== undefined && event.created_at < filter.since)
    return false;
  if (filter.until !== undefined && event.created_at > filter.until)
    return false;
  for (const [key, values] of Object.entries(filter)) {
    if (!key.startsWith("#") || !Array.isArray(values)) continue;
    const tagValues = values as string[];
    const tagName = key.slice(1);
    if (
      tagValues.length === 0 ||
      !event.tags.some(
        (tag) => tag[0] === tagName && tagValues.includes(tag[1] ?? ""),
      )
    ) {
      return false;
    }
  }
  return true;
}

/** Verify the canonical Nostr id and Schnorr signature before trusting relay data. */
export function isVerifiedRelayEvent(event: RelayEvent): boolean {
  if (import.meta.env?.MODE === "e2e" && event.sig.startsWith("mocksig")) {
    return true;
  }
  try {
    return verifyEvent({
      ...event,
      tags: event.tags.map((tag) => [...tag]),
    });
  } catch {
    return false;
  }
}
