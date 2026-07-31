import type { TimelineMessage } from "@/features/messages/types";
import { normalizePubkey } from "@/shared/lib/pubkey";

const PROJECT_WORKSPACE_PREFIX = "buzz://project-workspace?";

export type ProjectThreadContext = {
  localPath: string;
  repoAddress: string;
};

export type ProjectThreadAgentStep = {
  pubkey: string;
  status: "queued" | "working" | "done";
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
  agentPubkeys,
  replies,
}: {
  activeAgentPubkeys: readonly string[];
  agentPubkeys: readonly string[];
  replies: readonly TimelineMessage[];
}): ProjectThreadAgentStep[] {
  const active = new Set(activeAgentPubkeys.map(normalizePubkey));
  const replied = new Set(
    replies
      .filter((reply) => reply.isAgent)
      .map((reply) => normalizePubkey(reply.signerPubkey ?? reply.pubkey ?? ""))
      .filter(Boolean),
  );
  return [...new Set(agentPubkeys.map(normalizePubkey))]
    .filter(Boolean)
    .map((pubkey) => ({
      pubkey,
      status: active.has(pubkey)
        ? "working"
        : replied.has(pubkey)
          ? "done"
          : "queued",
    }));
}
