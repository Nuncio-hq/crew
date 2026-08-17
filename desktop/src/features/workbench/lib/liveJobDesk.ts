import { channelHrefFromWorkbench } from "./workbenchRoutes";
import type { WorkbenchAgentStatus } from "./workbenchThreadIndex";

export type LiveJobSignal =
  | { kind: "active-turn" }
  | { kind: "pending-user-input" }
  | { kind: "mission"; status: WorkbenchAgentStatus };

export type WorkbenchPlace =
  | { kind: "none" }
  | { kind: "channel-session"; channelId: string; threadRootId: string };

export function isLiveAgentJob(status: WorkbenchAgentStatus): boolean {
  switch (status) {
    case "working":
    case "needs-you":
      return true;
    case "sleeping":
    case "ready":
    case "idle":
    case "failed":
      return false;
    default: {
      const _exhaustive: never = status;
      return _exhaustive;
    }
  }
}

export function shouldShowLiveJobDesk(
  signals: readonly LiveJobSignal[],
): boolean {
  for (const signal of signals) {
    switch (signal.kind) {
      case "active-turn":
      case "pending-user-input":
        return true;
      case "mission":
        if (isLiveAgentJob(signal.status)) return true;
        break;
      default: {
        const _exhaustive: never = signal;
        return _exhaustive;
      }
    }
  }
  return false;
}

export function collectLiveJobSignals(args: {
  hasActiveTurn: boolean;
  hasPendingUserInput: boolean;
  missionStatus?: WorkbenchAgentStatus | null;
}): LiveJobSignal[] {
  const signals: LiveJobSignal[] = [];
  if (args.hasActiveTurn) signals.push({ kind: "active-turn" });
  if (args.hasPendingUserInput) signals.push({ kind: "pending-user-input" });
  if (args.missionStatus) {
    signals.push({ kind: "mission", status: args.missionStatus });
  }
  return signals;
}

export function resolveWorkbenchPlace(
  channelId?: string | null,
  threadRootId?: string | null,
): WorkbenchPlace {
  if (channelId && threadRootId) {
    return { kind: "channel-session", channelId, threadRootId };
  }
  return { kind: "none" };
}

export function hrefForWorkbenchPlace(place: WorkbenchPlace): string {
  switch (place.kind) {
    case "none":
      return "/";
    case "channel-session":
      return channelHrefFromWorkbench(place.channelId, place.threadRootId);
    default: {
      const _exhaustive: never = place;
      return _exhaustive;
    }
  }
}

export function selectedSessionFromLocation(location: {
  pathname: string;
  search: unknown;
}): { channelId: string | null; threadRootId: string | null } {
  const search =
    location.search && typeof location.search === "object"
      ? (location.search as { thread?: unknown; threadRootId?: unknown })
      : {};
  const threadFromSearch =
    (typeof search.threadRootId === "string" && search.threadRootId) ||
    (typeof search.thread === "string" && search.thread) ||
    null;
  const channelMatch = location.pathname.match(/^\/channels\/([^/]+)/);
  if (!channelMatch) {
    return { channelId: null, threadRootId: null };
  }
  return {
    channelId: decodeURIComponent(channelMatch[1] ?? ""),
    threadRootId: threadFromSearch,
  };
}
