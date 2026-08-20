/**
 * Returns a human-readable model label for an agent or persona, falling back to
 * "Auto" when no model is set (empty or whitespace-only). The literal Cursor /
 * shared-compute id `"auto"` also renders as "Auto".
 */
export function formatAgentModelLabel(model: string | null | undefined) {
  const trimmed = model?.trim();
  if (!trimmed) return "Auto";
  if (trimmed.toLowerCase() === "auto") return "Auto";
  return trimmed;
}
