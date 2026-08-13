import { normalizePubkey } from "@/shared/lib/pubkey";

export type ThreadAgentRef = {
  name: string;
  pubkey: string;
};

export function lastInteractingAgentPubkey(
  messages: ReadonlyArray<{ createdAt: number; pubkey?: string }>,
  agentPubkeys: ReadonlySet<string>,
): string | null {
  const agents = new Set(
    [...agentPubkeys].map((pubkey) => normalizePubkey(pubkey)),
  );
  let latest: { createdAt: number; pubkey: string } | null = null;
  for (const message of messages) {
    const pubkey = message.pubkey ? normalizePubkey(message.pubkey) : "";
    if (!pubkey || !agents.has(pubkey)) continue;
    if (!latest || message.createdAt >= latest.createdAt) {
      latest = { createdAt: message.createdAt, pubkey };
    }
  }
  return latest?.pubkey ?? null;
}

export function defaultComposerTarget(
  agents: readonly ThreadAgentRef[],
  lastInteractingPubkey: string | null,
): string | null {
  if (agents.length === 0) return null;
  const last = lastInteractingPubkey
    ? normalizePubkey(lastInteractingPubkey)
    : null;
  if (last && agents.some((agent) => normalizePubkey(agent.pubkey) === last)) {
    return last;
  }
  return normalizePubkey(agents[0].pubkey);
}

export function cycleComposerTarget(
  agents: readonly ThreadAgentRef[],
  currentPubkey: string | null,
): string | null {
  if (agents.length === 0) return null;
  const current = currentPubkey ? normalizePubkey(currentPubkey) : null;
  const index = agents.findIndex(
    (agent) => normalizePubkey(agent.pubkey) === current,
  );
  const next = agents[(index + 1) % agents.length];
  return normalizePubkey(next.pubkey);
}

/**
 * `@` in the composer body wins for that send. Otherwise the chip target
 * is appended so the wire format stays an ordinary mention (`p` tag).
 */
export function resolveSendTarget(args: {
  agentPubkeys: readonly string[];
  chipPubkey: string | null;
  mentionPubkeys: readonly string[];
}): string | null {
  const agents = new Set(
    args.agentPubkeys.map((pubkey) => normalizePubkey(pubkey)),
  );
  for (const mention of args.mentionPubkeys) {
    const pubkey = normalizePubkey(mention);
    if (agents.has(pubkey)) return pubkey;
  }
  return args.chipPubkey ? normalizePubkey(args.chipPubkey) : null;
}

export function mentionPubkeysForTarget(
  mentionPubkeys: readonly string[],
  targetPubkey: string | null,
): string[] {
  if (!targetPubkey) {
    return [...mentionPubkeys];
  }
  const target = normalizePubkey(targetPubkey);
  const seen = new Set<string>();
  const next: string[] = [];
  for (const mention of [target, ...mentionPubkeys]) {
    const pubkey = normalizePubkey(mention);
    if (!pubkey || seen.has(pubkey)) continue;
    seen.add(pubkey);
    next.push(pubkey);
  }
  return next;
}

export function agentDisplayName(
  agents: readonly ThreadAgentRef[],
  pubkey: string | null,
): string {
  if (!pubkey) return "agent";
  const key = normalizePubkey(pubkey);
  return (
    agents.find((agent) => normalizePubkey(agent.pubkey) === key)?.name ??
    "agent"
  );
}
