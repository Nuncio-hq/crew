import { PendingOriginPrompt } from "./OriginApprovalCard";
import { useAgentControlUi } from "./agentControlStore";
import { useToolPane } from "./toolPaneStore";

/** Origin elicitation while the Tool Pane is not already showing this card. */
export function ChannelOriginPrompt({ channelId }: { channelId: string }) {
  const ui = useAgentControlUi();
  const pane = useToolPane();
  if (ui.pendingOrigin?.channelId !== channelId) return null;
  if (pane.open && pane.tab === "browser") return null;
  return (
    <PendingOriginPrompt
      agentName={ui.pendingOrigin.agentName}
      channelId={channelId}
      origin={ui.pendingOrigin.origin}
    />
  );
}
