export type AgentReceiptLight = {
  label: string;
  status: string;
};

export type AgentReceiptEngineering = {
  prRef: string | null;
  branch: string | null;
  filesChanged: string[];
  ci: AgentReceiptLight[];
};

export type AgentReceiptModel = {
  summary: string;
  verify: string;
  lights: AgentReceiptLight[];
  engineering: AgentReceiptEngineering;
};

export function parseAgentReceipt(content: string): AgentReceiptModel | null;
