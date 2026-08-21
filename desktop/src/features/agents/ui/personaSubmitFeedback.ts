/**
 * Which Agents UI surface should show create/save feedback toasts.
 *
 * Create-from-catalog opens `PersonaCatalogDialog`; errors must land on the
 * catalog surface or the toast is swallowed while the modal stays open.
 * Standalone create / library edits use the library surface.
 */
export type PersonaFeedbackSurface = "catalog" | "library";

export function personaSubmitFeedbackSurface(
  isCatalogDialogOpen: boolean,
): PersonaFeedbackSurface {
  return isCatalogDialogOpen ? "catalog" : "library";
}

/** Copy when create cannot resolve an installed harness for `input.runtime`. */
export const PERSONA_CREATE_RUNTIME_UNAVAILABLE_MESSAGE =
  "Choose an available agent harness that is installed on this machine. Visit Settings → Agents to install or connect one.";

/**
 * When true, render the unavailable-runtime banner outside the harness picker.
 * Customize mode already attaches the warning to `AgentHarnessField`; defaults
 * mode (including Hermes preferred → tabs hidden) would otherwise leave Add
 * agent silently disabled.
 */
export function shouldShowStandaloneRuntimeUnavailableWarning(args: {
  isCreateMode: boolean;
  hasRuntimeWarning: boolean;
  aiConfigurationMode: "defaults" | "custom";
}): boolean {
  return (
    args.isCreateMode &&
    args.hasRuntimeWarning &&
    args.aiConfigurationMode !== "custom"
  );
}
