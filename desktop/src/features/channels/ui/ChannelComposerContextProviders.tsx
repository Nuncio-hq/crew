import { channelViewContext } from "@/features/messages/lib/crewViewContextModel";
import type { WorkspaceBindingChoice } from "@/features/messages/lib/workspaceBindingSpec";
import { ComposerViewContextProvider } from "@/features/messages/ui/composerViewContext";
import { ComposerWorkspaceBindingProvider } from "@/features/messages/ui/composerWorkspaceBinding";
import type { Channel } from "@/shared/api/types";

/**
 * Crew-owned composer context for the channel dock: the workspace binding
 * (#187) plus the visible-page agent context (#272). Both stay mounted in
 * Crew's channel-first chrome — no upstream Projects surface supplies them.
 */
export function ChannelComposerContextProviders({
  binding,
  channel,
  children,
}: {
  binding: WorkspaceBindingChoice;
  channel: Channel | null;
  children: React.ReactNode;
}) {
  return (
    <ComposerViewContextProvider
      value={channelViewContext(channel?.id ?? null, channel?.name ?? null)}
    >
      <ComposerWorkspaceBindingProvider value={binding}>
        {children}
      </ComposerWorkspaceBindingProvider>
    </ComposerViewContextProvider>
  );
}
