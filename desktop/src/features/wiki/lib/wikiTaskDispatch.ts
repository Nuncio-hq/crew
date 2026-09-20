/**
 * #367 — the renderer side of the durable Wiki task dispatch.
 *
 * The native commands own scope validation, membership re-checks, the signed
 * event, and the `OperationKind::ThreadHandoff` journal; this module only
 * shapes the intent payload and surfaces the bounded job projection the
 * panel renders. Every call passes the captured owner-operation token — a
 * workspace/identity switch between calls rejects instead of dispatching
 * under the wrong scope.
 */
import { invokeTauri } from "@/shared/api/tauri";
import type {
  OwnerOperationScope,
  ScopedOwnerOperation,
} from "@/shared/api/ownerOperations";

import type { PrivateAskDraftInput } from "@/features/wiki/lib/privateAskDev";
import type {
  WikiTaskDraftMeta,
  WikiTaskDraftReference,
} from "@/features/wiki/lib/wikiTaskDraft";

export type WikiTaskDispatchJob = {
  dispatchId: string;
  /** The pinned kickoff event id; set after prepare, stable across retries. */
  eventId: string | null;
  channelId: string;
  /** Accepted root id — the thread to open. Same value as `eventId`. */
  rootEventId: string | null;
  status:
    | "preparing"
    | "pending"
    | "reconciling"
    | "failed"
    | "complete"
    | "canceled"
    | "superseded";
  accepted: boolean;
  submitAttempts: number;
  error: string | null;
};

export type WikiTaskDispatchInput = {
  dispatchId: string;
  draftKey: string;
  questionId: string;
  attemptId: string;
  originCoordinate: string;
  sourceRevision: string | null;
  title: string;
  prompt: string;
  channelId: string;
  agentPubkey: string;
  references: { path: string; startLine: number; endLine: number }[];
};

/**
 * The exact intent the native command digests and signs. Only `included`
 * references are listed — an unchecked citation is private material that
 * never leaves this machine.
 */
export function buildDispatchInput({
  agentPubkey,
  channelId,
  draft,
  draftKey,
  meta,
  prompt,
}: {
  agentPubkey: string;
  channelId: string;
  draft: PrivateAskDraftInput;
  draftKey: string;
  meta: WikiTaskDraftMeta;
  prompt: string;
}): WikiTaskDispatchInput {
  const dispatchId = meta.dispatchId;
  if (!dispatchId) {
    throw new Error("the draft has no dispatch id to start");
  }
  return {
    dispatchId,
    draftKey,
    questionId: draft.questionId,
    attemptId: draft.attemptId,
    originCoordinate: draft.originCoordinate,
    sourceRevision: draft.sourceRevision,
    title: meta.title,
    prompt,
    channelId,
    agentPubkey,
    references: meta.references
      .filter((reference: WikiTaskDraftReference) => reference.included)
      .map((reference) => ({
        path: reference.path,
        startLine: reference.startLine,
        endLine: reference.endLine,
      })),
  };
}

export async function prepareWikiTaskDispatch(
  expected: OwnerOperationScope,
  input: WikiTaskDispatchInput,
): Promise<ScopedOwnerOperation<WikiTaskDispatchJob>> {
  return invokeTauri("wiki_task_dispatch_prepare", { expected, input });
}

export async function submitWikiTaskDispatch(
  expected: OwnerOperationScope,
  dispatchId: string,
): Promise<ScopedOwnerOperation<WikiTaskDispatchJob>> {
  return invokeTauri("wiki_task_dispatch_submit", { expected, dispatchId });
}

/** Resolve an ambiguous outcome by reading the relay — never republishes. */
export async function reconcileWikiTaskDispatch(
  expected: OwnerOperationScope,
  dispatchId: string,
): Promise<ScopedOwnerOperation<WikiTaskDispatchJob>> {
  return invokeTauri("wiki_task_dispatch_reconcile", { expected, dispatchId });
}

export async function abandonWikiTaskDispatch(
  expected: OwnerOperationScope,
  dispatchId: string,
  force: boolean,
): Promise<ScopedOwnerOperation<WikiTaskDispatchJob>> {
  return invokeTauri("wiki_task_dispatch_abandon", {
    expected,
    dispatchId,
    force,
  });
}

/**
 * Parse the `wiki:task:` draft key — the author-side backlink needs the
 * project route and attempt id the origin tag cannot carry (the event only
 * publishes the coordinate).
 */
export function parseWikiTaskDraftKey(key: string): {
  attemptId: string;
  projectId: string;
  repositoryCoordinate: string;
} | null {
  if (!key.startsWith("wiki:task:")) return null;
  const rest = key.slice("wiki:task:".length);
  const separator = rest.indexOf(":");
  if (separator <= 0) return null;
  const projectId = rest.slice(0, separator);
  const tail = rest.slice(separator + 1);
  // `<coordinate>` is `<owner-hex>:<repo-d>` — the last colon separates the
  // attempt id the key was minted for.
  const lastColon = tail.lastIndexOf(":");
  if (lastColon <= 0) return null;
  const repositoryCoordinate = tail.slice(0, lastColon);
  const attemptId = tail.slice(lastColon + 1);
  if (!projectId || !repositoryCoordinate.includes(":") || !attemptId) {
    return null;
  }
  return { attemptId, projectId, repositoryCoordinate };
}
