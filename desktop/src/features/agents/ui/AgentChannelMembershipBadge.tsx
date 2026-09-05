import { Badge } from "@/shared/ui/badge";

/** Channel readiness is separate from the process and direct-message readiness. */
export function AgentChannelMembershipBadge() {
  return (
    <Badge
      className="normal-case tracking-normal"
      variant="warning"
      title="Add this agent to a channel to receive channel work."
    >
      Running · No channels
    </Badge>
  );
}
