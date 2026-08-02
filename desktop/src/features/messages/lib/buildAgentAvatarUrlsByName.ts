import type { MentionCandidate } from "./mentionCandidates";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey } from "@/shared/lib/pubkey";

/** Deduplicate trimmed display names, preserving first-seen casing. */
export function uniqueTrimmedNames(names: string[]): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const name of names) {
    const trimmed = name.trim();
    if (trimmed && !seen.has(trimmed.toLowerCase())) {
      out.push(trimmed);
      seen.add(trimmed.toLowerCase());
    }
  }
  return out;
}

/**
 * Lowercased selected agent mention name → avatar URL for composer chips.
 * Prefers the live profile lookup; falls back to mention-candidate avatars.
 */
export function buildAgentAvatarUrlsByName({
  mentionCandidates,
  mentionMap,
  profiles,
  selectedAgentMentionNames,
}: {
  mentionCandidates: MentionCandidate[];
  mentionMap: Map<string, string>;
  profiles: UserProfileLookup | undefined;
  selectedAgentMentionNames: string[];
}): Record<string, string> {
  const values: Record<string, string> = {};
  const avatarByName = new Map<string, string>();

  for (const candidate of mentionCandidates) {
    const name = candidate.displayName?.trim();
    if (!name) {
      continue;
    }
    const isAgent =
      candidate.kind === "persona" ||
      candidate.kind === "team" ||
      candidate.isAgent === true;
    if (!isAgent) {
      continue;
    }
    const avatar =
      candidate.avatarUrl ??
      (candidate.pubkey
        ? (profiles?.[normalizePubkey(candidate.pubkey)]?.avatarUrl ?? null)
        : null);
    if (avatar) {
      avatarByName.set(name.toLowerCase(), avatar);
    }
  }

  for (const name of selectedAgentMentionNames) {
    const trimmed = name.trim();
    if (!trimmed) {
      continue;
    }
    const key = trimmed.toLowerCase();
    const pubkey = mentionMap.get(trimmed);
    const fromProfile = pubkey
      ? (profiles?.[normalizePubkey(pubkey)]?.avatarUrl ?? null)
      : null;
    const avatar = fromProfile ?? avatarByName.get(key) ?? null;
    if (avatar) {
      values[key] = avatar;
    }
  }

  return values;
}
