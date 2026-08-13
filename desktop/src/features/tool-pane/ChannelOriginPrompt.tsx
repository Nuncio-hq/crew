import { PendingOriginPrompt } from "./OriginApprovalCard";
import { useAgentControlUi } from "./agentControlStore";

/** Origin elicitation while the Tool Pane is closed (instrument ≠ pane). */
export function ChannelOriginPrompt({ channelId }: { channelId: string }) {
  const ui = useAgentControlUi();
  if (ui.pendingOrigin?.channelId !== channelId) return null;
  return (
    <PendingOriginPrompt
      agentName={ui.pendingOrigin.agentName}
      channelId={channelId}
      origin={ui.pendingOrigin.origin}
    />
  );
}
