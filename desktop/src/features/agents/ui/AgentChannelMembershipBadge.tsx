import { Badge } from "@/shared/ui/badge";
import type { ChannelMembershipState } from "../lib/channelMembershipState";

/** Channel readiness is separate from the process and direct-message readiness. */
export function AgentChannelMembershipBadge({
  state = "zero",
}: {
  state?: Extract<ChannelMembershipState, "unknown" | "zero">;
}) {
  const unknown = state === "unknown";
  return (
    <Badge
      className="normal-case tracking-normal"
      variant={unknown ? "secondary" : "warning"}
      title={
        unknown
          ? "Channel membership is not confirmed. Check the relay connection or restart this agent."
          : "Add this agent to a channel to receive channel work."
      }
    >
      {unknown ? "Running · Channels unknown" : "Running · No channels"}
    </Badge>
  );
}
