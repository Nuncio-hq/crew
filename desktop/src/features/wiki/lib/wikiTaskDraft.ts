/**
 * #367 — private Wiki task drafts.
 *
 * A Wiki task draft is editable private user state stored in the shared
 * scoped draft store (`useDrafts`) under an explicit `wiki:task:` namespace.
 * The draft key carries the full surface scope — project and repository
 * coordinate and the private-ask attempt it was opened from — while the
 * store itself already scopes entries to the relay community and viewer
 * pubkey. Saving publishes no event and wakes no agent.
 *
 * Because the draft lives in the shared map it inherits atomic write-back,
 * identity switching, and corruption tolerance from the production store.
 * `wiki:` keys get their own bounded partition so a Wiki save can never
 * evict an unrelated channel or thread composer draft (and vice versa); see
 * `WIKI_DRAFT_MAX_ENTRIES` in `useDrafts.ts`.
 */
import {
  clearDraftEntry,
  loadDraftEntry,
  saveDraftEntry,
  WIKI_DRAFT_MAX_ENTRIES,
  type DraftState,
} from "@/features/messages/lib/useDrafts";
import type {
  PrivateAskCitation,
  PrivateAskDraftInput,
} from "@/features/wiki/lib/privateAskDev";

export const WIKI_TASK_DRAFT_KIND = "crew-wiki-task" as const;
export const WIKI_TASK_DRAFT_META_VERSION = 1 as const;
/** Re-exported so callers/tests share the store partition bound. */
export const WIKI_TASK_DRAFT_MAX = WIKI_DRAFT_MAX_ENTRIES;

/**
 * Everything needed to rebuild the draft's storage key on reopen. The relay
 * community and viewer pubkey are NOT part of the key — the draft store
 * scopes them.
 */
export type WikiTaskDraftScope = {
  /**
   * The project route the Wiki pane is bound to, or `"library"` when the
   * pane is the company library. Stable project identity, not a label.
   */
  projectId: string;
  /** Bare `<owner-hex>:<repo-d>` coordinate the answer was grounded in. */
  repositoryCoordinate: string;
  /** The private-ask attempt this draft was opened from. */
  attemptId: string;
};

export function wikiTaskDraftKey(scope: WikiTaskDraftScope): string {
  return `wiki:task:${scope.projectId}:${scope.repositoryCoordinate}:${scope.attemptId}`;
}

export type WikiTaskDraftReference = {
  path: string;
  startLine: number;
  endLine: number;
  /** Only `included` references enter the channel message. */
  included: boolean;
};

/**
 * The structured fields of a Wiki task draft. Kept in `DraftState.meta` —
 * the draft's `content` holds the editable prompt text and `channelId` the
 * selected destination, so generic editor plumbing keeps working.
 */
export type WikiTaskDraftMeta = {
  kind: typeof WIKI_TASK_DRAFT_KIND;
  version: typeof WIKI_TASK_DRAFT_META_VERSION;
  /** Editable task title; may be empty while the user is still drafting. */
  title: string;
  /** Selected member-agent pubkey, or null while unchosen. */
  agentPubkey: string | null;
  /**
   * Where this draft came from — private, local-only identity of the ask
   * attempt. Never published; the channel event carries only the origin
   * coordinate.
   */
  origin: {
    questionId: string;
    attemptId: string;
    coordinate: string;
    sourceRevision: string | null;
    originAgent: string;
  };
  references: WikiTaskDraftReference[];
  /**
   * The durable dispatch operation id minted on the first Start. Persisted
   * with the draft so a retry after a crash resumes the same kickoff instead
   * of signing a second one.
   */
  dispatchId: string | null;
};

/** Title prefill: the question's first line, bounded for a subject field. */
const TITLE_PREFILL_MAX_CHARS = 80;

function deriveTitle(question: string): string {
  const firstLine = question
    .split("\n")
    .map((line) => line.trim())
    .find((line) => line.length > 0);
  if (!firstLine) return "";
  const chars = Array.from(firstLine);
  return chars.length > TITLE_PREFILL_MAX_CHARS
    ? `${chars.slice(0, TITLE_PREFILL_MAX_CHARS - 1).join("")}…`
    : firstLine;
}

/**
 * Build the fresh meta for a draft opened from a validated answer. All
 * citations start included — the user unchecks what should not be shared.
 */
export function newWikiTaskDraftMeta(
  input: PrivateAskDraftInput,
): WikiTaskDraftMeta {
  return {
    kind: WIKI_TASK_DRAFT_KIND,
    version: WIKI_TASK_DRAFT_META_VERSION,
    title: deriveTitle(input.question),
    agentPubkey: null,
    origin: {
      questionId: input.questionId,
      attemptId: input.attemptId,
      coordinate: input.originCoordinate,
      sourceRevision: input.sourceRevision,
      originAgent: input.originAgent,
    },
    references: input.citations.map(citationToReference),
    dispatchId: null,
  };
}

function citationToReference(
  citation: PrivateAskCitation,
): WikiTaskDraftReference {
  return {
    path: citation.path,
    startLine: citation.startLine,
    endLine: citation.endLine,
    included: true,
  };
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isValidReference(value: unknown): value is WikiTaskDraftReference {
  if (!isPlainObject(value)) return false;
  return (
    typeof value.path === "string" &&
    value.path.length > 0 &&
    typeof value.startLine === "number" &&
    typeof value.endLine === "number" &&
    typeof value.included === "boolean"
  );
}

/**
 * Read a stored `meta` blob as a Wiki task draft meta, or null when the
 * blob is absent/corrupt/foreign. Corruption yields null — the caller
 * treats the entry as no draft and may discard it explicitly.
 */
export function readWikiTaskDraftMeta(meta: unknown): WikiTaskDraftMeta | null {
  if (!isPlainObject(meta)) return null;
  if (meta.kind !== WIKI_TASK_DRAFT_KIND) return null;
  if (meta.version !== WIKI_TASK_DRAFT_META_VERSION) return null;
  if (typeof meta.title !== "string") return null;
  if (
    meta.agentPubkey !== null &&
    (typeof meta.agentPubkey !== "string" || meta.agentPubkey.length === 0)
  ) {
    return null;
  }
  if (!isPlainObject(meta.origin)) return null;
  const origin = meta.origin;
  if (
    typeof origin.questionId !== "string" ||
    typeof origin.attemptId !== "string" ||
    typeof origin.coordinate !== "string" ||
    (origin.sourceRevision !== null &&
      typeof origin.sourceRevision !== "string") ||
    typeof origin.originAgent !== "string"
  ) {
    return null;
  }
  if (
    !Array.isArray(meta.references) ||
    !meta.references.every(isValidReference)
  ) {
    return null;
  }
  if (
    meta.dispatchId !== null &&
    (typeof meta.dispatchId !== "string" || meta.dispatchId.length === 0)
  ) {
    return null;
  }
  return {
    kind: WIKI_TASK_DRAFT_KIND,
    version: WIKI_TASK_DRAFT_META_VERSION,
    title: meta.title,
    agentPubkey: meta.agentPubkey,
    origin: {
      questionId: origin.questionId,
      attemptId: origin.attemptId,
      coordinate: origin.coordinate,
      sourceRevision: origin.sourceRevision,
      originAgent: origin.originAgent,
    },
    references: meta.references.map((ref) => ({ ...ref })),
    dispatchId: meta.dispatchId,
  };
}

/** Read a draft entry as a Wiki task draft, or null when absent/corrupt. */
export function loadWikiTaskDraft(key: string): {
  meta: WikiTaskDraftMeta;
  prompt: string;
  channelId: string;
} | null {
  const entry = loadDraftEntry(key);
  if (!entry) return null;
  const meta = readWikiTaskDraftMeta(entry.meta);
  if (!meta) return null;
  return { meta, prompt: entry.content, channelId: entry.channelId };
}

/**
 * Persist a Wiki task draft atomically. `channelId` is the chosen
 * destination channel ("" while unchosen); `prompt` is the editable body.
 * The meta blob keeps title/agent/origin/references/dispatchId.
 */
export function saveWikiTaskDraft(
  key: string,
  meta: WikiTaskDraftMeta,
  prompt: string,
  channelId: string,
  options?: { updatedAt?: string },
): void {
  const now = options?.updatedAt ?? new Date().toISOString();
  const existing = loadDraftEntry(key);
  const draft: DraftState = {
    content: prompt,
    selectionStart: prompt.length,
    selectionEnd: prompt.length,
    channelId,
    createdAt: existing?.createdAt ?? now,
    updatedAt: options?.updatedAt ?? now,
    pendingImeta: [],
    spoileredAttachmentUrls: [],
    status: "active",
    meta: meta as unknown as Record<string, unknown>,
  };
  saveDraftEntry(key, draft);
}

/**
 * Remove the Wiki task draft after the user discards it or its dispatch is
 * accepted. Touches only this key — channel and thread drafts are unrelated
 * state and must survive.
 */
export function clearWikiTaskDraft(key: string): void {
  clearDraftEntry(key);
}
