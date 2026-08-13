import { parseWorkspaceBindingParams } from "@/features/messages/lib/workspaceBindingSpec";
import type { TimelineMessage } from "@/features/messages/types";
import { orderMentionPubkeysByText } from "@/features/messages/lib/orderMentionPubkeys";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey } from "@/shared/lib/pubkey";
import { resolveMentionProps } from "@/shared/lib/resolveMentionNames";

const PROJECT_WORKSPACE_PREFIX = "buzz://project-workspace?";

export type ProjectThreadContext = {
  localPath: string;
  repoAddress: string;
  ws: "new" | "main" | "branch";
  branch: string | null;
  base: string | null;
  mode: "git" | "folder";
};

export type ProjectThreadAgentStep = {
  pubkey: string;
  source: "root" | "reply";
  status: "queued" | "working" | "done";
};

export type ProjectThreadAgentMention = {
  pubkey: string;
  source: "root" | "reply";
};

export function projectThreadRootAudiencePubkeys(
  mentions: readonly ProjectThreadAgentMention[],
): string[] {
  return mentions
    .filter((mention) => mention.source === "root")
    .map((mention) => mention.pubkey);
}

/**
 * Whether the sticky project-thread status bar owns the agent activity signal,
 * so the composer must drop its duplicate.
 *
 * This has to track the bar's *actual* visibility. The bar renders only when it
 * has both a project context and at least one agent step, and steps are derived
 * solely from these mentions — so a project thread with no resolved mentions
 * shows no bar. Keying suppression on the context alone would hide the composer
 * line in exactly those cases, leaving a working agent with no indicator
 * anywhere. When the two disagree, prefer one extra indicator over none.
 */
export function projectThreadStickyBarOwnsAgentSignal(
  threadHeadBody: string | null | undefined,
  agentMentionCount: number,
): boolean {
  return (
    parseProjectThreadContext(threadHeadBody) != null && agentMentionCount > 0
  );
}

export function parseProjectThreadContext(
  content: string | null | undefined,
): ProjectThreadContext | null {
  const start = content?.indexOf(PROJECT_WORKSPACE_PREFIX) ?? -1;
  if (start < 0 || !content) return null;
  const suffix = content.slice(start);
  const end = suffix.search(/[>\s]/);
  const rawUrl = suffix.slice(0, end < 0 ? suffix.length : end);
  try {
    const url = new URL(rawUrl);
    const repoAddress = url.searchParams.get("repo")?.trim();
    const localPath = url.searchParams.get("path")?.trim();
    if (!repoAddress || !localPath?.startsWith("/")) return null;
    const parsed = parseWorkspaceBindingParams(
      url.searchParams.get("ws"),
      url.searchParams.get("base"),
    );
    const mode = url.searchParams.get("mode") === "folder" ? "folder" : "git";
    return { localPath, repoAddress, ...parsed, mode };
  } catch {
    return null;
  }
}

export function buildProjectThreadAgentSteps({
  activeAgentPubkeys,
  agentMentions,
  replies,
}: {
  activeAgentPubkeys: readonly string[];
  agentMentions: readonly ProjectThreadAgentMention[];
  replies: readonly TimelineMessage[];
}): ProjectThreadAgentStep[] {
  const active = new Set(activeAgentPubkeys.map(normalizePubkey));
  const replied = new Set(
    replies
      .filter((reply) => reply.isAgent)
      .map((reply) => normalizePubkey(reply.signerPubkey ?? reply.pubkey ?? ""))
      .filter(Boolean),
  );
  const orderedMentions = new Map<string, ProjectThreadAgentMention>();
  for (const mention of agentMentions) {
    const pubkey = normalizePubkey(mention.pubkey);
    if (pubkey && !orderedMentions.has(pubkey)) {
      orderedMentions.set(pubkey, { ...mention, pubkey });
    }
  }
  return [...orderedMentions.values()].map(({ pubkey, source }) => ({
    pubkey,
    source,
    status: active.has(pubkey)
      ? "working"
      : replied.has(pubkey)
        ? "done"
        : "queued",
  }));
}

export function collectProjectThreadAgentMentions({
  knownAgentPubkeys,
  profiles,
  replies,
  threadHead,
}: {
  knownAgentPubkeys: ReadonlySet<string>;
  profiles?: UserProfileLookup;
  replies: readonly TimelineMessage[];
  threadHead: TimelineMessage;
}): ProjectThreadAgentMention[] {
  const seen = new Set<string>();
  const mentions: ProjectThreadAgentMention[] = [];
  const messages = [
    { message: threadHead, source: "root" as const },
    ...replies.map((message) => ({ message, source: "reply" as const })),
  ];
  for (const { message, source } of messages) {
    const { mentionPubkeysByName } = resolveMentionProps(
      message.tags,
      profiles,
    );
    const ordered = orderMentionPubkeysByText(
      message.body,
      mentionPubkeysByName,
      (pubkey) =>
        knownAgentPubkeys.has(pubkey) || profiles?.[pubkey]?.isAgent === true,
    );
    for (const pubkey of ordered) {
      if (seen.has(pubkey)) continue;
      seen.add(pubkey);
      mentions.push({ pubkey, source });
    }
  }
  return mentions;
}
