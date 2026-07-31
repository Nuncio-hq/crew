import { sendAgentObserverControl } from "@/shared/api/observerRelay";
import { invokeTauri } from "@/shared/api/tauri";
import type {
  CancelManagedAgentTurnResult,
  ThreadWorkspaceActionResult,
} from "@/shared/api/types";

export async function cancelManagedAgentTurn(
  pubkey: string,
  channelId: string,
  conversationId: string,
  turnId: string,
): Promise<CancelManagedAgentTurnResult> {
  await sendAgentObserverControl(pubkey, {
    type: "cancel_turn",
    channelId,
    conversationId,
    turnId,
  });
  return { status: "sent" };
}

/**
 * Send a live model-switch control frame to a running agent. The switch rides
 * the harness's cancel-switch-requeue path (busy turn) or invalidate-and-reapply
 * (idle); the outcome arrives asynchronously as a `control_result` observer
 * frame, not as the return value here. This is fire-and-forget on the send side.
 */
export async function switchManagedAgentModel(
  pubkey: string,
  channelId: string,
  conversationId: string,
  turnId: string,
  modelId: string,
): Promise<void> {
  await sendAgentObserverControl(pubkey, {
    type: "switch_model",
    channelId,
    conversationId,
    turnId,
    modelId,
  });
}

type ThreadWorkspaceTarget = {
  repositoryPath: string;
  branch: string;
  rootEventId: string;
};

export function removeThreadWorktree(
  input: ThreadWorkspaceTarget & { worktreePath: string },
): Promise<ThreadWorkspaceActionResult> {
  return invokeTauri("remove_thread_worktree", input);
}

export function deleteThreadBranch(
  input: ThreadWorkspaceTarget,
): Promise<ThreadWorkspaceActionResult> {
  return invokeTauri("delete_thread_branch", input);
}

export function closeThreadPullRequest(
  input: ThreadWorkspaceTarget,
): Promise<ThreadWorkspaceActionResult> {
  return invokeTauri("close_thread_pull_request", input);
}
