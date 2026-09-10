import * as React from "react";

import { normalizePubkey } from "@/shared/lib/pubkey";
import type { ManagedAgent } from "@/shared/api/types";
import type { ObserverEvent } from "../ui/agentSessionTypes";

type Membership = {
  generation: string;
  startedAt: number;
  seq: number;
  count: number | null;
};
/** Current projection state; absence and an explicit null count are unknown. */
export type ChannelMembershipState = "unknown" | "zero" | "nonzero";
const membershipByAgent = new Map<string, Membership>();
const listeners = new Set<() => void>();
const notify = () => {
  for (const listener of listeners) listener();
};

function stateForCount(
  count: number | null | undefined,
): ChannelMembershipState {
  if (count === null || count === undefined) return "unknown";
  return count === 0 ? "zero" : "nonzero";
}

function stateForMembership(
  membership: Membership | undefined,
): ChannelMembershipState {
  return stateForCount(membership?.count);
}

/** Project the harness subscription snapshot, independently of conversation sessions. */
export function applyChannelMembershipObserverFrame(
  agentPubkey: string,
  frame: Pick<ObserverEvent, "kind" | "payload" | "seq">,
): void {
  if (frame.kind !== "channel_membership" || !agentPubkey.trim()) return;
  const payload = frame.payload;
  if (!payload || typeof payload !== "object") return;
  const {
    channel_count: count,
    generation,
    generation_started_at: started,
  } = payload as Record<string, unknown>;
  if (
    (count !== null &&
      (typeof count !== "number" ||
        !Number.isSafeInteger(count) ||
        count < 0)) ||
    typeof generation !== "string" ||
    !generation.trim() ||
    typeof started !== "string" ||
    !Number.isSafeInteger(frame.seq) ||
    frame.seq < 0
  )
    return;
  const normalizedCount = count as number | null;
  const startedAt = Date.parse(started);
  if (!Number.isFinite(startedAt)) return;
  const key = normalizePubkey(agentPubkey);
  const previous = membershipByAgent.get(key);
  if (
    previous &&
    (generation === previous.generation
      ? startedAt !== previous.startedAt || frame.seq <= previous.seq
      : startedAt <= previous.startedAt)
  )
    return;
  membershipByAgent.set(key, {
    generation,
    startedAt,
    seq: frame.seq,
    count: normalizedCount,
  });
  if (stateForMembership(previous) !== stateForCount(normalizedCount)) notify();
}

export function getChannelMembershipState(
  agentPubkey: string | null | undefined,
): ChannelMembershipState {
  if (!agentPubkey?.trim()) return "unknown";
  return stateForMembership(
    membershipByAgent.get(normalizePubkey(agentPubkey)),
  );
}

export function hasNoChannelMembership(
  agentPubkey: string | null | undefined,
): boolean {
  return !!agentPubkey && getChannelMembershipState(agentPubkey) === "zero";
}

export function subscribeChannelMembershipState(
  listener: () => void,
): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useChannelMembershipState(
  agentPubkey: string | null | undefined,
): ChannelMembershipState {
  return React.useSyncExternalStore(subscribeChannelMembershipState, () =>
    getChannelMembershipState(agentPubkey),
  );
}

/** Community boundaries discard both projection and harness generation authority. */
export function resetChannelMembershipState(): void {
  membershipByAgent.clear();
  notify();
}

export function deriveNoChannelMembershipBadge(
  signal: boolean,
  status: ManagedAgent["status"],
): boolean {
  return signal && (status === "running" || status === "deployed");
}

export function useNoChannelMembership(
  agentPubkey: string | null | undefined,
): boolean {
  return useChannelMembershipState(agentPubkey) === "zero";
}
