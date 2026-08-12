import {
  CHANNEL_AUX_EVENT_KINDS,
  CHANNEL_EVENT_KINDS,
  KIND_CHANNEL_THREAD_SUMMARY,
  CHANNEL_TIMELINE_CONTENT_KINDS,
  HOME_MENTION_EVENT_KINDS,
  KIND_DELETION,
  KIND_AGENT_USER_INPUT_ANSWER,
  KIND_AGENT_USER_INPUT_REQUESTED,
  KIND_AGENT_USER_INPUT_RESOLVED,
  KIND_NIP29_DELETE_EVENT,
  KIND_REACTION,
  KIND_STREAM_MESSAGE,
  KIND_STREAM_MESSAGE_V2,
  KIND_STREAM_MESSAGE_EDIT,
} from "@/shared/constants/kinds";
import type { RelaySubscriptionFilter } from "@/shared/api/relayClientShared";
import { CHANNEL_LIVE_BACKLOG_GRACE_SECONDS } from "@/shared/api/relayClientTimings";

// Auxiliary-event backfill: `#e` filters reference loaded message ids to pull
// their reactions/edits/deletions. Chunk the ids so each REQ stays within
// relay filter limits, and let each chunk return up to the relay's WS cap —
// a single reaction-heavy message can have many aux events.
export const AUX_BACKFILL_CHUNK_SIZE = 100;
export const MAX_HISTORICAL_LIMIT = 10_000;

/**
 * Window-store live filter for an open channel: rows, aux, and the kind:39005
 * thread-summary recounts that ride only this subscription (no other consumer
 * may see summary overlays).
 *
 * A relay matches `since` against `created_at`, so a bound of exactly "now"
 * silently drops two classes of event the timeline must not miss: one published
 * by a peer whose clock lags ours, and one created between the window fetch that
 * seeded the timeline and this subscription opening. The client never learns
 * such an event exists, so nothing short of a refetch recovers it.
 * {@link CHANNEL_LIVE_BACKLOG_GRACE_SECONDS} trades a small replay for that gap
 * — the window store merges by event id, so a replayed event it already holds is
 * dropped and one it does not is exactly the event that would have been lost.
 */
export function buildChannelLiveFilter(
  channelId: string,
  nowSeconds: number,
): RelaySubscriptionFilter {
  return {
    kinds: [...CHANNEL_EVENT_KINDS, KIND_CHANNEL_THREAD_SUMMARY],
    "#h": [channelId],
    limit: 1000,
    since: nowSeconds - CHANNEL_LIVE_BACKLOG_GRACE_SECONDS,
  };
}

export function buildChannelUserInputFilter(
  channelId: string,
  limit = 200,
  since?: number,
): RelaySubscriptionFilter {
  const filter: RelaySubscriptionFilter = {
    kinds: [
      KIND_AGENT_USER_INPUT_REQUESTED,
      KIND_AGENT_USER_INPUT_ANSWER,
      KIND_AGENT_USER_INPUT_RESOLVED,
    ],
    "#h": [channelId],
    limit,
  };
  if (since !== undefined) filter.since = since;
  return filter;
}

/**
 * Live-subscription filter for an open channel: the broad
 * {@link CHANNEL_EVENT_KINDS} set so the tail delivers reactions/edits/
 * deletions for future messages as well as new message rows.
 */
export function buildChannelFilter(
  channelId: string,
  limit: number,
  until?: number,
): RelaySubscriptionFilter {
  const filter: RelaySubscriptionFilter = {
    kinds: [...CHANNEL_EVENT_KINDS],
    "#h": [channelId],
    limit,
  };

  if (until !== undefined) {
    filter.until = until;
  }

  return filter;
}

/**
 * Huddle TTS message filter with a bounded startup replay window.
 *
 * The Huddle window and agent membership snapshot can finish mounting just
 * after the first agent reply is stored. Replaying only the caller-provided
 * startup window closes that race; event-id dedup in the consumer prevents a
 * stored row from being spoken twice when it is also delivered live.
 */
export function buildHuddleTtsLiveFilter(
  channelId: string,
  since: number,
): RelaySubscriptionFilter {
  return {
    kinds: [KIND_STREAM_MESSAGE, KIND_STREAM_MESSAGE_V2],
    "#h": [channelId],
    since,
    limit: 50,
  };
}

/**
 * History filter for cold-load and scrollback: message kinds *only*, so the
 * `limit` budget buys visible message depth. Auxiliary events (reactions,
 * edits, deletions) are backfilled separately by `#e` reference via
 * {@link buildChannelStructuralAuxFilter} and
 * {@link buildChannelReactionAuxFilter}, and arrive for future messages
 * through the live subscription ({@link buildChannelFilter}, which keeps the
 * broad {@link CHANNEL_EVENT_KINDS} set).
 */
export function buildChannelHistoryFilter(
  channelId: string,
  limit: number,
  until?: number,
): RelaySubscriptionFilter {
  const filter: RelaySubscriptionFilter = {
    kinds: [...CHANNEL_TIMELINE_CONTENT_KINDS],
    "#h": [channelId],
    limit,
  };

  if (until !== undefined) {
    filter.until = until;
  }

  return filter;
}

/**
 * Aux-backfill filter for one chunk of loaded message ids: pulls auxiliary
 * events ({@link CHANNEL_AUX_EVENT_KINDS}) that reference those ids by `#e`.
 * Keyed by reference, not time, so a late edit/deletion for an old visible
 * message still applies — see {@link buildChannelHistoryFilter}.
 */
export function buildChannelAuxFilter(
  _channelId: string,
  messageIds: string[],
): RelaySubscriptionFilter {
  return buildChannelAuxKindFilter(messageIds, [...CHANNEL_AUX_EVENT_KINDS]);
}

/**
 * Structural aux filter for history backfill: edits/deletions only. Reactions
 * are hydrated from the rows the GUI actually renders, so the slow kind:5 scan
 * never shares a request with first-paint reaction pills.
 */
export function buildChannelStructuralAuxFilter(
  _channelId: string,
  messageIds: string[],
): RelaySubscriptionFilter {
  return buildChannelAuxKindFilter(messageIds, [
    KIND_DELETION,
    KIND_NIP29_DELETE_EVENT,
    KIND_STREAM_MESSAGE_EDIT,
  ]);
}

/**
 * Reactions-only filter for the message rows the GUI is currently rendering.
 * Keep this separate from structural aux backfill so the slow kind:5 deletion
 * scan cannot delay reaction pills that affect visible pixels right now.
 */
export function buildChannelReactionAuxFilter(
  _channelId: string,
  messageIds: string[],
): RelaySubscriptionFilter {
  return buildChannelAuxKindFilter(messageIds, [KIND_REACTION]);
}

export function buildChannelAuxDeletionFilter(
  _channelId: string,
  auxEventIds: string[],
): RelaySubscriptionFilter {
  return buildChannelAuxKindFilter(auxEventIds, [
    KIND_DELETION,
    KIND_NIP29_DELETE_EVENT,
  ]);
}

// No `#h`: reaction/reaction-removal events carry only an `e` tag, so an
// `#h`-scoped query misses them; `#e` over unique ids is already specific.
function buildChannelAuxKindFilter(
  referencedEventIds: string[],
  kinds: number[],
): RelaySubscriptionFilter {
  return {
    kinds,
    "#e": referencedEventIds,
    limit: MAX_HISTORICAL_LIMIT,
  };
}

export function buildGlobalStreamFilter(
  limit: number,
): RelaySubscriptionFilter {
  return {
    kinds: [...CHANNEL_EVENT_KINDS],
    limit,
  };
}

export function buildChannelMentionFilter(
  channelId: string,
  pubkey: string,
  limit: number,
): RelaySubscriptionFilter {
  return {
    kinds: [...HOME_MENTION_EVENT_KINDS],
    "#h": [channelId],
    "#p": [pubkey],
    limit,
    since: Math.floor(Date.now() / 1_000),
  };
}
