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
  run: {
    sessionId: string;
    turnId: string;
  } | null;
};

export function parseAgentReceipt(content: string): AgentReceiptModel | null;
