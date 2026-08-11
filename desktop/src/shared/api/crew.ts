export type AssignChannelAgentRoleInput = {
  channelId: string;
  agentPubkey: string;
  label: string;
  definition: string;
  overwriteForeignCanvas?: boolean;
};

export type CanvasRoleAssignment = {
  agentPubkey: string;
  roleLabel: string;
};

export type CanvasRoutingPreset = {
  workType: string;
  roleLabel: string;
  holders: string[];
  unheldMessage: string | null;
};
