import { sendAgentObserverControl } from "@/shared/api/observerRelay";
import { invokeTauri } from "@/shared/api/tauri";
import type {
  ClearProjectWorktreeCacheResult,
  ProjectWorktreeDetails,
  ProjectWorktreeReclaimPreview,
  ProjectWorktreeRegistry,
  ReclaimTier,
  ThreadGitHubStatus,
  ThreadWorkspaceActionResult,
  ThreadWorkspaceLifecycle,
  WorktreeStorageAliveStatus,
  WorktreeStorageSnapshot,
} from "@/shared/api/thread-workspace-types";

/**
 * Ask the harness to stop work for a conversation.
 *
 * `turnId` is optional: omit it (or pass null) during the dispatch-hold window
 * when nothing is in flight yet — Phase 1 drains the queued batch and reports
 * `cancelled_queued` via a `control_result` observer frame. The sync return
 * here only means the control event was published; listen for `control_result`
 * for the real outcome.
 */
export async function cancelManagedAgentTurn(
  pubkey: string,
  channelId: string,
  conversationId: string,
  turnId?: string | null,
  requestId?: string,
): Promise<void> {
  await sendAgentObserverControl(pubkey, {
    type: "cancel_turn",
    channelId,
    conversationId,
    ...(turnId ? { turnId } : {}),
    ...(requestId ? { requestId } : {}),
  });
}

/**
 * Ask the harness to re-dispatch failed events from a failure notice.
 *
 * Never publish a fresh kind-9 with the same body — that duplicates the
 * person's message and re-notifies mentions. Outcome arrives as
 * `control_result` with `type: "retry_turn"`.
 */
export async function retryManagedAgentTurn(
  pubkey: string,
  channelId: string,
  conversationId: string,
  eventIds: readonly string[],
): Promise<void> {
  await sendAgentObserverControl(pubkey, {
    type: "retry_turn",
    channelId,
    conversationId,
    eventIds: [...eventIds],
  });
}

/**
 * Send a live model-switch control frame to a running agent. The switch rides
 * the harness's cancel-switch-requeue path (busy turn) or invalidate-and-reapply
 * (idle); the outcome arrives asynchronously as a `control_result` observer
 * frame, not as the return value here. This is fire-and-forget on the send side.
 *
 * `requestId` is an opaque per-pick correlator the harness echoes back on both
 * the immediate ack and the late terminal frame, so a reconnect replay of an
 * earlier pick's result cannot settle this one.
 */
export async function switchManagedAgentModel(
  pubkey: string,
  channelId: string,
  conversationId: string,
  turnId: string,
  modelId: string,
  requestId: string,
): Promise<void> {
  await sendAgentObserverControl(pubkey, {
    type: "switch_model",
    channelId,
    conversationId,
    turnId,
    modelId,
    requestId,
  });
}

/**
 * Owner-triggered guided handover (#173): one-shot summarizer → handover note
 * on the thread → session/new with OwnerReset. Outcome arrives as
 * `control_result` (`guided_handover`). On summarizer failure the frame sets
 * `allowBlindReset` so the owner can call {@link blindSessionResetManagedAgent}.
 */
export async function guidedHandoverManagedAgent(
  pubkey: string,
  channelId: string,
  conversationId: string,
  options?: {
    modelId?: string | null;
    rootEventId?: string | null;
    latestOwnerMessage?: string;
  },
): Promise<void> {
  await sendAgentObserverControl(pubkey, {
    type: "guided_handover",
    channelId,
    conversationId,
    ...(options?.modelId ? { modelId: options.modelId } : {}),
    ...(options?.rootEventId ? { rootEventId: options.rootEventId } : {}),
    ...(options?.latestOwnerMessage
      ? { latestOwnerMessage: options.latestOwnerMessage }
      : {}),
  });
}

/** Blind session reset when guided handover's summarizer path fails (#173). */
export async function blindSessionResetManagedAgent(
  pubkey: string,
  channelId: string,
  conversationId: string,
): Promise<void> {
  await sendAgentObserverControl(pubkey, {
    type: "blind_session_reset",
    channelId,
    conversationId,
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

export function getThreadWorkspaceLifecycle(
  input: ThreadWorkspaceTarget & { worktreePath: string },
): Promise<ThreadWorkspaceLifecycle> {
  return invokeTauri("get_thread_workspace_lifecycle", input);
}

export function getThreadGitHubStatus(
  input: ThreadWorkspaceTarget,
): Promise<ThreadGitHubStatus> {
  return invokeTauri("get_thread_github_status", input);
}

export function getProjectWorktreeRegistry(
  repositoryPath: string,
): Promise<ProjectWorktreeRegistry> {
  return invokeTauri("get_project_worktree_registry", { repositoryPath });
}

export function getProjectWorktreeDetails(
  repositoryPath: string,
  worktreePath: string,
): Promise<ProjectWorktreeDetails> {
  return invokeTauri("get_project_worktree_details", {
    repositoryPath,
    worktreePath,
  });
}

export function previewProjectWorktreeReclaim(
  repositoryPath: string,
  worktreePath: string,
  expectedRoutingChannelId?: string | null,
): Promise<ProjectWorktreeReclaimPreview> {
  return invokeTauri("preview_project_worktree_reclaim", {
    repositoryPath,
    worktreePath,
    expectedRoutingChannelId: expectedRoutingChannelId ?? null,
  });
}

export function clearProjectWorktreeCache(
  repositoryPath: string,
  worktreePath: string,
  categoryIds: string[],
  expectedRoutingChannelId: string,
): Promise<ClearProjectWorktreeCacheResult> {
  return invokeTauri("clear_project_worktree_cache", {
    repositoryPath,
    worktreePath,
    categoryIds,
    expectedRoutingChannelId,
  });
}

export function removeProjectWorktree(
  repositoryPath: string,
  worktreePath: string,
  expectedRoutingChannelId: string,
): Promise<ThreadWorkspaceActionResult> {
  return invokeTauri("remove_project_worktree", {
    repositoryPath,
    worktreePath,
    expectedRoutingChannelId,
  });
}

export function evictProjectWorktree(
  repositoryPath: string,
  worktreePath: string,
  expectedRoutingChannelId: string,
): Promise<ThreadWorkspaceActionResult> {
  return invokeTauri("evict_project_worktree", {
    repositoryPath,
    worktreePath,
    expectedRoutingChannelId,
  });
}

export function pruneProjectWorktrees(
  repositoryPath: string,
): Promise<ThreadWorkspaceActionResult> {
  return invokeTauri("prune_project_worktrees", { repositoryPath });
}

export function touchWorktreeStorageAlive(): Promise<WorktreeStorageAliveStatus> {
  return invokeTauri("touch_worktree_storage_alive");
}

export function getWorktreeStorageAlive(): Promise<WorktreeStorageAliveStatus> {
  return invokeTauri("get_worktree_storage_alive");
}

export function setWorktreeStorageIdleThreshold(
  idleThresholdSecs: number,
): Promise<WorktreeStorageAliveStatus> {
  return invokeTauri("set_worktree_storage_idle_threshold", {
    idleThresholdSecs,
  });
}

export function getWorktreeStorageSnapshot(
  repositoryPaths: string[],
  idleThresholdSecs?: number | null,
): Promise<WorktreeStorageSnapshot> {
  return invokeTauri("get_worktree_storage_snapshot", {
    repositoryPaths,
    idleThresholdSecs: idleThresholdSecs ?? null,
  });
}

export function revalidateWorktreeStorageAction(
  repositoryPath: string,
  worktreePath: string,
  expectedRoutingChannelId: string,
  tier: ReclaimTier,
): Promise<string | null> {
  return invokeTauri("revalidate_worktree_storage_action", {
    repositoryPath,
    worktreePath,
    expectedRoutingChannelId,
    tier,
  });
}
