import type { TimelineMessage } from "@/features/messages/types";
import { orderMentionPubkeysByText } from "@/features/messages/lib/orderMentionPubkeys";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey } from "@/shared/lib/pubkey";
import { resolveMentionProps } from "@/shared/lib/resolveMentionNames";

const PROJECT_WORKSPACE_PREFIX = "buzz://project-workspace?";

export type ProjectThreadContext = {
  localPath: string;
  repoAddress: string;
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
    return { localPath, repoAddress };
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
