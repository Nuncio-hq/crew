import {
  beginExhaustiveApprovalProjection,
  endExhaustiveApprovalProjection,
} from "@/features/agents/needsYouStore";

export type ApprovalSubscriptionRegistry = {
  approvalRequest: Array<() => Promise<void>>;
  approvalTerminal: Array<() => Promise<void>>;
};

export function failApprovalSubscriptions(
  subscriptions: ApprovalSubscriptionRegistry,
) {
  const disposers = [
    ...subscriptions.approvalRequest,
    ...subscriptions.approvalTerminal,
  ];
  subscriptions.approvalRequest = [];
  subscriptions.approvalTerminal = [];
  void Promise.allSettled(disposers.map((dispose) => dispose()));
  const owner = beginExhaustiveApprovalProjection();
  endExhaustiveApprovalProjection(owner, false);
}
