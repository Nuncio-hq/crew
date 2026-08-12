export type ControlResultFrame = {
  type:
    | "cancel_turn"
    | "switch_model"
    | "retry_turn"
    | "guided_handover"
    | "blind_session_reset";
  status: string;
  modelId?: string;
  conversationId?: string | null;
  turnId?: string | null;
  dispatchedCount?: number;
  withheldCount?: number;
  requestedCount?: number;
  /** Guided handover degradation (#173): owner may still blind-reset. */
  allowBlindReset?: boolean;
  error?: string | null;
};
