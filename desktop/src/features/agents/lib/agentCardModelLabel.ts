import { formatAgentModelLabel } from "./formatAgentModelLabel";
import { profileCardModelLabel } from "./hermesProfileBinding";
import type { ManagedAgent } from "@/shared/api/types";

/**
 * Model label for the Agents-page card, shared by the definition-grouped
 * card (`AgentPersonaCard`) and the standalone/legacy card
 * (`StandaloneAgentCard`).
 *
 * A materialized `agent` is authoritative once it exists: its `modelSource`
 * says whether the *effective* config (from `resolve_effective_config` on
 * the backend) came from the global default or from an explicit
 * definition/instance value, so the card never has to re-derive that from
 * raw model bytes. Absent or `"global"` renders the default-model label;
 * anything else renders the agent's own resolved model.
 *
 * Hermes profile-bound agents never inherit Crew global defaults for display —
 * the profile owns the runtime model (feature 0001 S-2.2).
 *
 * With NO materialized instance yet (a definition that has never been
 * started), there is no `modelSource` to read — the definition itself is
 * authoritative, so an explicit `personaModel` must render directly rather
 * than falling through to "inherited" for lack of an instance.
 */
export function resolveAgentCardModelLabel(input: {
  agent:
    | Pick<ManagedAgent, "modelSource" | "model" | "hermesProfile">
    | undefined;
  personaModel: string | null | undefined;
  defaultModel: string;
  hermesProfile?: string | null;
  /** From `read_hermes_profile_model` when the card should show disk config. */
  profileModelFromDisk?: string | null;
}): string {
  const hermesProfile =
    input.agent?.hermesProfile?.trim() || input.hermesProfile?.trim() || null;
  if (hermesProfile) {
    return profileCardModelLabel(
      hermesProfile,
      input.profileModelFromDisk ?? input.agent?.model,
    );
  }

  if (input.agent) {
    const isInherited =
      !input.agent.modelSource || input.agent.modelSource === "global";
    if (isInherited) {
      return formatDefaultModelLabel(input.defaultModel);
    }
    return input.agent.model?.trim()
      ? formatAgentModelLabel(input.agent.model)
      : formatDefaultModelLabel(input.defaultModel);
  }
  return input.personaModel?.trim()
    ? formatAgentModelLabel(input.personaModel)
    : formatDefaultModelLabel(input.defaultModel);
}

export function formatDefaultModelLabel(defaultModel: string) {
  const model = defaultModel.trim();
  return model ? `Default model (${model})` : "Default model";
}
