/**
 * Sidebar rail contract for NuncioCrew (#223).
 *
 * The rail is Inbox + channels + DMs. A Project (NIP-MP 30621 / repo group)
 * is not a sidebar peer. Exclusive repo bindings do not pull a channel out
 * of Channels into a second tree.
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
