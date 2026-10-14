/**
 * Stream-channel rail contract for NuncioCrew (#223).
 *
 * Recorded before D-078: the channel stream stays channels + DMs. Exclusive
 * repo bindings do not pull a channel out of the stream into a second tree.
 * D-078 / #344 adds a separate Projects sidebar group (Wiki + channels per
 * Project) alongside this stream; it does not change this filter.
 */

export type SidebarStreamChannel = {
  channelType: string;
};

/**
 * Stream channels shown under Channels. `projectFolderIds` is accepted so
 * callers can pass the work-tree set; it is ignored — office channels stay
 * in the list.
 */
export function streamChannelsForSidebar<T extends SidebarStreamChannel>(
  channels: readonly T[],
  _projectFolderIds?: ReadonlySet<string>,
): T[] {
  return channels.filter((channel) => channel.channelType === "stream");
}
