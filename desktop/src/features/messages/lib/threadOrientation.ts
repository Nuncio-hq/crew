import type { TimelineMessage } from "@/features/messages/types";

export type ThreadBreadcrumbSegment = {
  /** Ancestor or head message this segment stands for. Carried whole so the
   *  ancestry strip can reopen it without a second lookup. */
  message: TimelineMessage;
  author: string;
  /** One-line preview; always filled when the body has text (phase 3 uses
   *  every segment; the breadcrumb UI only renders the terminal one). */
  snippet: string | null;
};

export type ThreadBreadcrumb = {
  channelName: string;
  /** Top-level ancestor first, thread head last. Never empty. */
  segments: ThreadBreadcrumbSegment[];
  /** True when ancestors were dropped to satisfy the segment cap. */
  truncated: boolean;
  /** The message the main timeline should anchor on: always top-level. */
  anchorMessageId: string;
  anchorMessage: TimelineMessage | null;
};

const MAX_WALK_HOPS = 8;
const MAX_SEGMENTS = 3;
const SNIPPET_MAX_CHARS = 40;

/** Strip fenced code, collapse whitespace, truncate on a word boundary. */
export function buildMessageSnippet(
  body: string | null | undefined,
): string | null {
  if (body == null) return null;
  const withoutFences = body.replace(/```[\s\S]*?```/g, " ");
  const collapsed = withoutFences.replace(/\s+/g, " ").trim();
  if (collapsed.length === 0) return null;
  if (collapsed.length <= SNIPPET_MAX_CHARS) return collapsed;

  const slice = collapsed.slice(0, SNIPPET_MAX_CHARS);
  const lastSpace = slice.lastIndexOf(" ");
  const cut =
    lastSpace > Math.floor(SNIPPET_MAX_CHARS / 2)
      ? slice.slice(0, lastSpace)
      : slice;
  return `${cut}…`;
}

function toSegment(message: TimelineMessage): ThreadBreadcrumbSegment {
  return {
    message,
    author: message.author,
    snippet: buildMessageSnippet(message.body),
  };
}

/**
 * Build the orientation breadcrumb for an open thread panel.
 * Returns null when the head or channel name is missing — callers fall back
 * to the literal "Thread" title.
 */
export function buildThreadBreadcrumb(input: {
  channelName: string | null | undefined;
  threadHead: TimelineMessage | null;
  messageById: ReadonlyMap<string, TimelineMessage>;
}): ThreadBreadcrumb | null {
  const channelName = input.channelName?.trim() ?? "";
  const { threadHead, messageById } = input;
  if (!threadHead || channelName.length === 0) {
    return null;
  }

  const chain: TimelineMessage[] = [];
  let current: TimelineMessage | null = threadHead;
  let hops = 0;

  // Walk parentId upward. Do NOT stop on depth === 0 alone: the thread panel
  // normalizes every head to depth 0 (`normalizeHeadMessage`) even when the
  // message still has a parentId (nested open). Missing/null parentId is the
  // real top-level signal.
  while (current && hops < MAX_WALK_HOPS) {
    chain.push(current);
    const parentId = current.parentId;
    if (!parentId) break;
    const parent = messageById.get(parentId);
    if (!parent) break;
    // Cycle guard: parent already in the chain.
    if (chain.some((entry) => entry.id === parent.id)) break;
    current = parent;
    hops += 1;
  }

  // Walk collects head-first; reverse so top-level ancestor is first.
  chain.reverse();

  const topLevel = chain[0];
  const reachedTopLevel = !topLevel.parentId;
  const anchorMessageId = reachedTopLevel
    ? topLevel.id
    : (threadHead.rootId ?? threadHead.id);
  const anchorMessage = messageById.get(anchorMessageId) ?? null;

  let truncated = false;
  let segments: ThreadBreadcrumbSegment[];
  if (chain.length <= MAX_SEGMENTS) {
    segments = chain.map(toSegment);
  } else {
    // Keep the first (timeline anchor) and the last two (immediate context).
    truncated = true;
    segments = [
      toSegment(chain[0]),
      toSegment(chain[chain.length - 2]),
      toSegment(chain[chain.length - 1]),
    ];
  }

  return {
    channelName,
    segments,
    truncated,
    anchorMessageId,
    anchorMessage,
  };
}
