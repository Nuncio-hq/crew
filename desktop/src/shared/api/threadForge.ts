import { invokeTauri } from "./tauri";
import type {
  ForgeActionResult,
  ForgeCheckLogResult,
  ForgeDetailResult,
  ForgeDiffResult,
  ForgeMergeStrategy,
  ForgeReviewEvent,
} from "./threadForgeTypes";

export function getThreadForgePrDetail(input: {
  owner: string;
  name: string;
  number: number;
}): Promise<ForgeDetailResult> {
  return invokeTauri("get_thread_forge_pr_detail", input);
}

export function getThreadForgePrDiff(input: {
  owner: string;
  name: string;
  number: number;
  worktreePath?: string | null;
  baseRef?: string | null;
}): Promise<ForgeDiffResult> {
  return invokeTauri("get_thread_forge_pr_diff", input);
}

export function getForgeCheckLogTail(input: {
  owner: string;
  name: string;
  number: number;
  runId: number;
}): Promise<ForgeCheckLogResult> {
  return invokeTauri("get_forge_check_log_tail", input);
}

export function rerunForgeChecks(input: {
  owner: string;
  name: string;
  number: number;
  runId: number;
  failedOnly: boolean;
}): Promise<ForgeActionResult> {
  return invokeTauri("rerun_forge_checks", input);
}

export function commentForgePr(input: {
  owner: string;
  name: string;
  number: number;
  body: string;
}): Promise<ForgeActionResult> {
  return invokeTauri("comment_forge_pr", input);
}

export function reviewForgePr(input: {
  owner: string;
  name: string;
  number: number;
  event: ForgeReviewEvent;
  body: string;
}): Promise<ForgeActionResult> {
  return invokeTauri("review_forge_pr", input);
}

export function mergeForgePr(input: {
  owner: string;
  name: string;
  number: number;
  strategy: ForgeMergeStrategy;
}): Promise<ForgeActionResult> {
  return invokeTauri("merge_forge_pr", input);
}

export function setForgeFileViewed(input: {
  owner: string;
  name: string;
  number: number;
  pullRequestId: string;
  path: string;
  viewed: boolean;
}): Promise<ForgeActionResult> {
  return invokeTauri("set_forge_file_viewed", input);
}

export function resolveForgePrByUrl(url: string): Promise<ForgeDetailResult> {
  return invokeTauri("resolve_forge_pr_by_url", { url });
}

export function createForgePr(input: {
  owner: string;
  name: string;
  worktreePath: string;
  title: string;
  body: string;
  base: string;
  head?: string | null;
}): Promise<ForgeActionResult> {
  return invokeTauri("create_forge_pr", input);
}
