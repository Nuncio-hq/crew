/**
 * Public-path runtime faces that `<img>` and mention-chip CSS can load.
 * Keep in sync with RuntimeIcon: compiled-in runtimes, presets, and the
 * catalog aliases on those ids. SVG React marks (Claude/Goose/Cursor) have
 * matching public files because chat/mentions cannot render those components.
 */
const RUNTIME_BITMAP_AVATARS: Record<string, string> = {
  hermes: "/harness-logos/hermes.png",
  "hermes-agent": "/harness-logos/hermes.png",
  claude: "/harness-logos/claude.png",
  "claude-code": "/harness-logos/claude.png",
  claudecode: "/harness-logos/claude.png",
  goose: "/harness-logos/goose.svg",
  cursor: "/harness-logos/cursor.svg",
  codex: "/harness-logos/terminal.svg",
  kimi: "/harness-logos/kimi.png",
  amp: "/harness-logos/amp.png",
  devin: "/harness-logos/devin.svg",
  omp: "/harness-logos/omp.svg",
  grok: "/harness-logos/grok.svg",
  opencode: "/harness-logos/opencode.svg",
  openclaw: "/harness-logos/openclaw.svg",
};

/**
 * Resolve the avatar for a running agent card.
 *
 * The card opens the concrete agent pubkey's profile, so that profile's kind:0
 * picture is authoritative. The linked definition remains a fallback while the
 * profile is missing or has no picture. Chat, DMs, and mentions use `<img>`,
 * so the last candidate is the runtime's bundled bitmap (Hermes PNG, preset
 * logos) — the same face Agents page shows via RuntimeIcon when no picture is
 * set.
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

function effectiveRuntimeId(
  agentRuntime?: string | null,
  personaRuntime?: string | null,
): string | null {
  const fromAgent = agentRuntime?.trim() || "";
  if (fromAgent.length > 0 && fromAgent !== "custom") return fromAgent;
  const fromPersona = personaRuntime?.trim() || "";
  if (fromPersona.length > 0 && fromPersona !== "custom") return fromPersona;
  return null;
}

/** Public bitmap/SVG URL for a runtime default face, or null if unknown. */
export function resolveRuntimeDefaultAvatarUrl(
  runtimeId: string | null | undefined,
): string | null {
  const id = runtimeId?.trim().toLowerCase() ?? "";
  if (!id || id === "custom") return null;
  return RUNTIME_BITMAP_AVATARS[id] ?? null;
}

/** Map of persona id → default avatar, used when kind:0 has no picture. */
export function personaAvatarById(
  personas: readonly { id: string; avatarUrl: string | null }[],
): Record<string, string | null> {
  return Object.fromEntries(
    personas.map((persona) => [persona.id, persona.avatarUrl]),
  );
}

/** Map of persona id → harness id, used when the instance inherits runtime. */
export function personaRuntimeById(
  personas: readonly { id: string; runtime: string | null }[],
): Record<string, string | null> {
  return Object.fromEntries(
    personas.map((persona) => [persona.id, persona.runtime]),
  );
}

type PersonaAvatarSource = {
  id: string;
  avatarUrl: string | null;
  runtime: string | null;
};

type ManagedAgentAvatarSource = {
  pubkey: string;
  avatarUrl?: string | null;
  runtime?: string | null;
  personaId?: string | null;
};

/** Kind:0 picture, then instance, then persona, then runtime bitmap. */
export function resolveManagedAgentDisplayAvatarUrl({
  profileAvatarUrl,
  agentAvatarUrl,
  agentRuntime,
  personaAvatarUrl,
  personaRuntime,
}: {
  profileAvatarUrl?: string | null;
  agentAvatarUrl?: string | null;
  agentRuntime?: string | null;
  personaAvatarUrl?: string | null;
  personaRuntime?: string | null;
}): string | null {
  return resolveAgentCardAvatarUrl(
    profileAvatarUrl,
    agentAvatarUrl,
    personaAvatarUrl,
    resolveRuntimeDefaultAvatarUrl(
      effectiveRuntimeId(agentRuntime, personaRuntime),
    ),
  );
}

export function mentionAvatarForManagedAgent(
  agent: ManagedAgentAvatarSource,
  personas: readonly PersonaAvatarSource[],
  profileAvatarUrl?: string | null,
): string | null {
  const persona = agent.personaId
    ? personas.find((item) => item.id === agent.personaId)
    : undefined;
  return resolveManagedAgentDisplayAvatarUrl({
    profileAvatarUrl,
    agentAvatarUrl: agent.avatarUrl,
    agentRuntime: agent.runtime,
    personaAvatarUrl: persona?.avatarUrl,
    personaRuntime: persona?.runtime,
  });
}

export function mentionAvatarForPersona(
  persona: Pick<PersonaAvatarSource, "avatarUrl" | "runtime">,
): string | null {
  return resolveManagedAgentDisplayAvatarUrl({
    personaAvatarUrl: persona.avatarUrl,
    personaRuntime: persona.runtime,
  });
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
