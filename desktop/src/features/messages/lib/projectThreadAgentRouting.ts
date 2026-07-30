import { normalizePubkey } from "@/shared/lib/pubkey";

const PROJECT_WORKSPACE_CONTEXT = "buzz://project-workspace?";

type RoutingInput = {
  content: string;
  explicitAgentPubkeys: string[];
  isThreadReply: boolean;
  mentionPubkeys: string[];
};

export type ProjectThreadAgentRouting = {
  mentionPubkeys: string[];
  referencePubkeys: string[];
};

/**
 * A new Project task wakes only the first explicitly ordered agent. Remaining
 * agents stay renderable as non-notifying references and can be handed work in
 * later thread replies. Ordinary chat, non-Project channels, and replies keep
 * their existing mention behavior.
 */
export function resolveProjectThreadAgentRouting({
  content,
  explicitAgentPubkeys,
  isThreadReply,
  mentionPubkeys,
}: RoutingInput): ProjectThreadAgentRouting {
  const orderedAgents = uniquePubkeys(explicitAgentPubkeys);
  if (
    isThreadReply ||
    !content.includes(PROJECT_WORKSPACE_CONTEXT) ||
    orderedAgents.length < 2
  ) {
    return { mentionPubkeys, referencePubkeys: [] };
  }

  const deferred = new Set(orderedAgents.slice(1));
  return {
    mentionPubkeys: mentionPubkeys.filter(
      (pubkey) => !deferred.has(normalizePubkey(pubkey)),
    ),
    referencePubkeys: [...deferred],
  };
}

function uniquePubkeys(pubkeys: Iterable<string>) {
  return [...new Set([...pubkeys].map(normalizePubkey))].filter(Boolean);
}
