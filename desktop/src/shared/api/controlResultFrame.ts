export type ControlResultFrame = {
  type: "cancel_turn" | "switch_model" | "retry_turn";
  status: string;
  modelId?: string;
  conversationId?: string | null;
  turnId?: string | null;
  dispatchedCount?: number;
  withheldCount?: number;
  requestedCount?: number;
};
