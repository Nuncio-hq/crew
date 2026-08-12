/** Crew handover note tag: `["crew-handover", <model-id>]` (#173). */

export function parseHandoverModel(
  tags: readonly (readonly string[])[] | undefined,
): string | null {
  const value = tags?.find((tag) => tag[0] === "crew-handover")?.[1];
  if (!value || value.trim().length === 0) {
    return null;
  }
  return value;
}
