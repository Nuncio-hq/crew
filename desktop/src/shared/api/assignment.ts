import { invokeTauri } from "./tauri";
import type { AssignChannelAgentRoleInput } from "./types";

type RawAssignChannelAgentRoleResult = {
  ok: boolean;
  canvas_event_id: string;
  announcement_event_id: string;
};

export async function assignChannelAgentRole(
  input: AssignChannelAgentRoleInput,
): Promise<RawAssignChannelAgentRoleResult> {
  return invokeTauri<RawAssignChannelAgentRoleResult>(
    "assign_channel_agent_role",
    {
      channelId: input.channelId,
      agentPubkey: input.agentPubkey,
      label: input.label,
      definition: input.definition,
    },
  );
}
