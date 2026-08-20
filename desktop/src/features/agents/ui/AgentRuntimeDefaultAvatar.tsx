import type { AcpRuntimeCatalogEntry } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { RuntimeIcon } from "@/features/onboarding/ui/RuntimeIcon";
import { IdentityInitialsAvatar } from "./IdentityInitialsAvatar";

/** Minimal catalog stub — `RuntimeIcon` only reads `id`. */
export function runtimeIconStub(
  runtimeId: string,
): Pick<AcpRuntimeCatalogEntry, "id"> {
  return { id: runtimeId.trim() };
}

/**
 * Effective runtime id for default agent marks: instance pin, else persona.
 */
export function resolveAgentDefaultRuntimeId({
  agentRuntime,
  personaRuntime,
}: {
  agentRuntime?: string | null;
  personaRuntime?: string | null;
}): string | null {
  const fromAgent = agentRuntime?.trim() || "";
  if (fromAgent.length > 0 && fromAgent !== "custom") return fromAgent;
  const fromPersona = personaRuntime?.trim() || "";
  if (fromPersona.length > 0 && fromPersona !== "custom") return fromPersona;
  return null;
}

type AgentRuntimeDefaultAvatarProps = {
  className?: string;
  /** Icon box size in CSS pixels (square). */
  iconClassName?: string;
  label: string;
  runtimeId?: string | null;
  size: number;
};

/**
 * Default agent face when no custom avatar URL is set: the runtime's SVG/PNG
 * mark (Cursor → Cursor mark, Goose → Goose mark, …), else initials.
 */
export function AgentRuntimeDefaultAvatar({
  className,
  iconClassName,
  label,
  runtimeId,
  size,
}: AgentRuntimeDefaultAvatarProps) {
  const id = runtimeId?.trim() || "";
  if (!id || id === "custom") {
    return (
      <IdentityInitialsAvatar className={className} label={label} size={size} />
    );
  }

  const iconSizeClass =
    iconClassName ??
    (size >= 96 ? "h-12 w-12" : size >= 64 ? "h-8 w-8" : "h-5 w-5");

  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-flex items-center justify-center overflow-hidden rounded-full bg-muted text-foreground",
        className,
      )}
      data-testid="agent-runtime-default-avatar"
      data-runtime-id={id}
      style={{ height: size, width: size }}
      title={label}
    >
      <RuntimeIcon className={iconSizeClass} runtime={runtimeIconStub(id)} />
    </span>
  );
}
