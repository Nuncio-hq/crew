import { compareObserverEvents } from "@/features/agents/observerRelayStore";
import type { ObserverEvent } from "./ui/agentSessionTypes";

const NULL_CHANNEL_KEY = "\u0000null-channel";

function watermarkChannelKey(event: ObserverEvent): string {
  return event.channelId ?? NULL_CHANNEL_KEY;
}

// Composite watermark per (agent, channel): the newest observer event
// processed for that channel, by (timestamp, seq) ordering. An event is
// processed only if it is strictly newer than its channel's watermark,
// making full-buffer replays idempotent and post-restart streams (seq
// resets to 1, timestamp keeps climbing) handled for free.
const lastProcessed = new Map<string, Map<string, ObserverEvent>>();

/**
 * Return the previous event for this (agent, channel) pair if it is still
 * current enough that the given `event` should be ignored, or `undefined`
 * if the event should be processed.
 */
export function gateEventByWatermark(
  agentKey: string,
  event: ObserverEvent,
): ObserverEvent | undefined {
  const channelKey = watermarkChannelKey(event);
  const agentWatermarks = lastProcessed.get(agentKey);
  const last = agentWatermarks?.get(channelKey);
  if (last && compareObserverEvents(event, last) <= 0) {
    return last;
  }
  return undefined;
}

/** Record that `event` has been processed for `agentKey`. */
export function recordEventProcessed(agentKey: string, event: ObserverEvent) {
  const channelKey = watermarkChannelKey(event);
  let agentWatermarks = lastProcessed.get(agentKey);
  if (!agentWatermarks) {
    agentWatermarks = new Map();
    lastProcessed.set(agentKey, agentWatermarks);
  }
  agentWatermarks.set(channelKey, event);
}

export function clearTurnsWatermarks() {
  lastProcessed.clear();
}

export function snapshotTurnsWatermarks(): Map<
  string,
  Map<string, ObserverEvent>
> {
  const watermarks = new Map<string, Map<string, ObserverEvent>>();
  for (const [agentKey, channelMarks] of lastProcessed) {
    watermarks.set(agentKey, new Map(channelMarks));
  }
  return watermarks;
}

export function restoreTurnsWatermarks(
  watermarks: Map<string, Map<string, ObserverEvent>>,
) {
  lastProcessed.clear();
  for (const [agentKey, channelMarks] of watermarks) {
    lastProcessed.set(agentKey, new Map(channelMarks));
  }
}
