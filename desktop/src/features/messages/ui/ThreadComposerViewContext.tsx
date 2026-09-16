import type { TimelineMessage } from "@/features/messages/types";
import { threadViewContext } from "@/features/messages/lib/crewViewContextModel";
import { ComposerViewContextProvider } from "./composerViewContext";
import type { ProjectThreadWorkspaceModel } from "./useProjectThreadWorkspaceModel";

/**
 * Crew-owned mount for the visible-page agent context of thread focus mode
 * (#272). The payload describes what the sender is looking at — the thread, its
 * channel, and the workspace entities Crew's own thread chrome already shows —
 * so no upstream Projects surface is needed to supply it.
 */
export function ThreadComposerViewContext({
  channelId,
  channelName,
  children,
  model,
  threadHead,
}: {
  channelId: string | null;
  channelName: string;
  children: React.ReactNode;
  model: ProjectThreadWorkspaceModel | null;
  threadHead: TimelineMessage;
}) {
  return (
    <ComposerViewContextProvider
      value={threadViewContext({
        branch: model?.target?.branch ?? null,
        channelId,
        channelName,
        pullRequest: model?.pullRequest ?? null,
        repositoryPath: model?.target?.repositoryPath ?? null,
        threadRootId: threadHead.id,
        threadTitle: threadHead.body ?? "",
      })}
    >
      {children}
    </ComposerViewContextProvider>
  );
}
