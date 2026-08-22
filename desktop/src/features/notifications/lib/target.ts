import { getThreadReference } from "@/features/messages/lib/threading";
import type { FeedItem, RelayEvent } from "@/shared/api/types";
import type { DesktopNotificationTarget } from "./desktop";

export function buildEventNotificationTarget(
  event: Pick<
    RelayEvent,
    "content" | "created_at" | "id" | "kind" | "pubkey" | "tags"
  >,
  channel: { id: string; name?: string | null },
): DesktopNotificationTarget {
  return {
    channelId: channel.id,
    channelName: channel.name?.trim() || null,
    content: event.content,
    createdAt: event.created_at,
    eventId: event.id,
    kind: event.kind,
    pubkey: event.pubkey,
    threadRootId: getThreadReference(event.tags).rootId ?? null,
  };
}

export function buildFeedItemNotificationTarget(
  item: FeedItem,
): DesktopNotificationTarget {
  return {
    channelId: item.channelId,
    channelName: item.channelName,
    content: item.content,
    createdAt: item.createdAt,
    eventId: item.id,
    kind: item.kind,
    pubkey: item.pubkey,
    threadRootId: getThreadReference(item.tags).rootId ?? null,
  };
}
