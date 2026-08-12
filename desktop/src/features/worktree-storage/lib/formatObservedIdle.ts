/** Dual-clock copy: "idle 52 observed hrs (last used 9 days ago)". */
export function formatObservedIdleLine(input: {
  observedIdleSecs: number;
  wallIdleSecs: number | null;
}): string {
  const observedHrs = Math.max(0, Math.floor(input.observedIdleSecs / 3600));
  const observed =
    observedHrs <= 0
      ? "idle <1 observed hr"
      : `idle ${observedHrs} observed hr${observedHrs === 1 ? "" : "s"}`;
  if (input.wallIdleSecs == null) {
    return observed;
  }
  return `${observed} (last used ${formatWallAge(input.wallIdleSecs)} ago)`;
}

export function formatWallAge(wallIdleSecs: number): string {
  const secs = Math.max(0, wallIdleSecs);
  const days = Math.floor(secs / 86_400);
  if (days >= 1) {
    return `${days} day${days === 1 ? "" : "s"}`;
  }
  const hours = Math.floor(secs / 3600);
  if (hours >= 1) {
    return `${hours} hour${hours === 1 ? "" : "s"}`;
  }
  const mins = Math.max(1, Math.floor(secs / 60));
  return `${mins} min`;
}

export function formatAbsenceBanner(recentAbsenceSecs: number): string | null {
  const days = Math.floor(recentAbsenceSecs / 86_400);
  if (days < 2) return null;
  return `You were away ${days} days — in-progress threads are unlikely to qualify yet`;
}

export function repositoryLabel(repositoryPath: string): string {
  const trimmed = repositoryPath.replace(/\/+$/u, "");
  const parts = trimmed.split("/");
  return parts[parts.length - 1] || trimmed || "repository";
}
