import { useChannelsQuery } from "@/features/channels/hooks";
import type { Channel } from "@/shared/api/types";

/**
 * Thin Crew adapter for upstream compact-link openability (#5638/#6252).
 *
 * Upstream's `openChannelDirectory` is a bounded non-member detail lookup.
 * Crew does not port that directory (thin-fork / #289 scope). Member-list
 * lookup is enough for chips: known channels resolve immediately; unknown
 * ids stay the shortened-id fallback.
 */
export function isChannelReferenceOpenable(
  channel: Channel | undefined,
): channel is Channel {
  return Boolean(
    channel && (channel.isMember || channel.visibility === "open"),
  );
}

export function useChannelReference(channelId: string): Channel | undefined {
  const { data } = useChannelsQuery();
  return data?.find((channel) => channel.id === channelId);
}

export function useResolvedChannelDirectory(): { channels: Channel[] } {
  const { data } = useChannelsQuery();
  return { channels: data ?? [] };
}
