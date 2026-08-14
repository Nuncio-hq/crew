import { WORK_TREE_QUIET_MS } from "./workTreeTypes";

export function formatCompactAge(msAgo: number): string {
  if (!Number.isFinite(msAgo) || msAgo < 0) return "now";
  const minutes = Math.floor(msAgo / 60_000);
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

export function formatFolderQuietAge(
  lastActivityAt: number,
  now: number,
): string | null {
  const ago = now - lastActivityAt;
  if (ago < WORK_TREE_QUIET_MS) return null;
  return formatCompactAge(ago);
}

export function truncateMiddle(value: string, max = 28): string {
  if (value.length <= max) return value;
  const keep = Math.max(1, Math.floor((max - 1) / 2));
  return `${value.slice(0, keep)}…${value.slice(-keep)}`;
}
