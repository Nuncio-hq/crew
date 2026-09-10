import type { ThreadForgeHubSubject } from "@/features/messages/lib/threadForgeHubSubjectStore";

/** URL subjects have the same exact conversation boundary as discovered PRs. */
export function matchingThreadToolPaneSubject(
  subject: ThreadForgeHubSubject | null | undefined,
  channelId: string | null,
  rootEventId: string | null | undefined,
): ThreadForgeHubSubject | null {
  return channelId &&
    rootEventId &&
    subject?.channelId === channelId &&
    subject.rootEventId === rootEventId
    ? subject
    : null;
}
