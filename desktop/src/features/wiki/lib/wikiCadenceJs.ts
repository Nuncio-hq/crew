/** Mirrors `crew_wiki::cadence` so the desktop worker can schedule without FFI. */

export const ON_PUSH_DEBOUNCE_SECS = 30;

export function debounce_due(
  lastFiredUnix: number,
  lastPushUnix: number,
  nowUnix: number,
): boolean {
  if (lastPushUnix <= 0) return false;
  if (nowUnix < lastPushUnix + ON_PUSH_DEBOUNCE_SECS) return false;
  return lastFiredUnix < lastPushUnix;
}

export function next_cadence_due(
  cadence: "manual" | "on-push" | "daily" | "weekly",
  lastGeneratedUnix: number,
  nowUnix: number,
): boolean {
  if (cadence === "daily") return nowUnix - lastGeneratedUnix >= 86_400;
  if (cadence === "weekly") return nowUnix - lastGeneratedUnix >= 86_400 * 7;
  return false;
}
