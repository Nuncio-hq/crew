/** Pure ESM mirror of handoverTag.ts for node:test. */

export function parseHandoverModel(tags) {
  const value = tags?.find((tag) => tag[0] === "crew-handover")?.[1];
  if (!value || value.trim().length === 0) {
    return null;
  }
  return value;
}
