import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey } from "@/shared/lib/pubkey";
import type { ThreadAgentRef } from "./workbenchComposerTarget";

export function collectThreadAgents(args: {
  knownAgentPubkeys: ReadonlySet<string>;
  managedAgents: ReadonlyArray<{ name: string; pubkey: string }>;
  messages: readonly TimelineMessage[];
  profiles?: UserProfileLookup;
}): ThreadAgentRef[] {
  const known = new Set(
    [...args.knownAgentPubkeys].map((pubkey) => normalizePubkey(pubkey)),
  );
  const names = new Map<string, string>();
  for (const agent of args.managedAgents) {
    names.set(normalizePubkey(agent.pubkey), agent.name);
  }
  if (args.profiles) {
    for (const [pubkey, profile] of Object.entries(args.profiles)) {
      const name = profile.displayName?.trim();
      if (name) names.set(normalizePubkey(pubkey), name);
    }
  }

  const ordered: string[] = [];
  const seen = new Set<string>();
  const consider = (raw: string | undefined) => {
    const pubkey = raw ? normalizePubkey(raw) : "";
    if (!pubkey || seen.has(pubkey) || !known.has(pubkey)) return;
    seen.add(pubkey);
    ordered.push(pubkey);
  };

  for (const message of args.messages) {
    consider(message.pubkey);
    consider(message.signerPubkey);
    for (const tag of message.tags ?? []) {
      if (tag[0] === "p") consider(tag[1]);
    }
  }

  return ordered.map((pubkey) => ({
    name: names.get(pubkey) ?? pubkey.slice(0, 8),
    pubkey,
  }));
}
