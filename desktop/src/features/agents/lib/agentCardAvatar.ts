/**
 * Resolve the avatar for a running agent card.
 *
 * The card opens the concrete agent pubkey's profile, so that profile's kind:0
 * picture is authoritative. The linked definition remains a fallback while the
 * profile is missing or has no picture.
 */
export function resolveAgentCardAvatarUrl(
  ...candidates: Array<string | null | undefined>
): string | null {
  for (const candidate of candidates) {
    const trimmed = candidate?.trim();
    if (trimmed) return trimmed;
  }
  return null;
}

/** Map of persona id → default avatar, used when kind:0 has no picture. */
export function personaAvatarById(
  personas: readonly { id: string; avatarUrl: string | null }[],
): Record<string, string | null> {
  return Object.fromEntries(
    personas.map((persona) => [persona.id, persona.avatarUrl]),
  );
}

/**
 * A linked agent's profile is authoritative even when the definition already
 * supplies a fallback. Avatar-dependent actions must wait for that profile
 * query so they cannot snapshot the fallback before the profile resolves.
 */
export function isAgentCardAvatarLoading(
  hasLinkedAgent: boolean,
  isProfilePending: boolean,
): boolean {
  return hasLinkedAgent && isProfilePending;
}
