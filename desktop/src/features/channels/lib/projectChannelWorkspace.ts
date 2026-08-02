import { parseProjectThreadContext } from "@/features/messages/lib/projectThreadWorkspace";
import type { TimelineMessage } from "@/features/messages/types";

export type ProjectChannelWorkspace = {
  repositoryPath: string | null;
  channelRootIds: Set<string>;
  rootBodiesById: Map<string, string>;
};

/** Derive the project repo path and top-level root ids from the channel timeline. */
export function deriveProjectChannelWorkspace(
  timelineMessages: readonly TimelineMessage[],
): ProjectChannelWorkspace {
  let repositoryPath: string | null = null;
  const channelRootIds = new Set<string>();
  const rootBodiesById = new Map<string, string>();
  for (const message of timelineMessages) {
    if (message.parentId) continue;
    channelRootIds.add(message.id);
    rootBodiesById.set(message.id.toLowerCase(), message.body);
    if (!repositoryPath && message.body.includes("buzz://project-workspace?")) {
      repositoryPath =
        parseProjectThreadContext(message.body)?.localPath ?? null;
    }
  }
  return { repositoryPath, channelRootIds, rootBodiesById };
}
