/** Pure ESM mirror of sessionAgingStore.ts for node:test (no React / path aliases). */

function normalizePubkey(pubkey) {
  return String(pubkey).toLowerCase();
}

const entries = new Map();

function agingKey(agentPubkey, conversationId) {
  return `${normalizePubkey(agentPubkey)}:${conversationId}`;
}

export function putSessionAging(entry) {
  const key = agingKey(entry.agentPubkey, entry.conversationId);
  if (!entry.aging) {
    entries.delete(key);
    return;
  }
  entries.set(key, {
    ...entry,
    agentPubkey: normalizePubkey(entry.agentPubkey),
    compactionCount:
      entry.compactionSignal === "known" ? entry.compactionCount : 0,
  });
}

export function clearSessionAging(agentPubkey, conversationId) {
  entries.delete(agingKey(agentPubkey, conversationId));
}

export function clearAllSessionAging() {
  entries.clear();
}

export function getSessionAging(agentPubkey, conversationId) {
  return entries.get(agingKey(agentPubkey, conversationId));
}

export function parseSessionAgingPayload(agentPubkey, payload) {
  if (!payload || typeof payload !== "object") {
    return null;
  }
  const channelId =
    typeof payload.channelId === "string" ? payload.channelId : null;
  const conversationId =
    typeof payload.conversationId === "string"
      ? payload.conversationId
      : channelId;
  if (!channelId || !conversationId) {
    return null;
  }
  const signalRaw =
    typeof payload.compactionSignal === "string"
      ? payload.compactionSignal
      : "unknown";
  const compactionSignal =
    signalRaw === "known"
      ? "known"
      : signalRaw === "unavailable"
        ? "unavailable"
        : "unknown";
  return {
    agentPubkey: normalizePubkey(
      typeof payload.pubkey === "string" ? payload.pubkey : agentPubkey,
    ),
    channelId,
    conversationId,
    aging: payload.aging === true,
    reason:
      typeof payload.reason === "string"
        ? payload.reason
        : payload.aging === true
          ? "compaction_threshold"
          : null,
    compactionCount:
      typeof payload.compactionCount === "number" ? payload.compactionCount : 0,
    compactionSignal,
    sessionTurnCount:
      typeof payload.sessionTurnCount === "number"
        ? payload.sessionTurnCount
        : 0,
    compactionThreshold:
      typeof payload.compactionThreshold === "number"
        ? payload.compactionThreshold
        : 3,
    turnThreshold:
      typeof payload.turnThreshold === "number" ? payload.turnThreshold : 100,
  };
}

export function sessionAgingBannerText(entry, agentLabel) {
  if (entry.reason === "turn_count_net") {
    return `${agentLabel}'s long session (${entry.sessionTurnCount}+ turns)`;
  }
  if (entry.compactionSignal === "known" && entry.compactionCount > 0) {
    return `${agentLabel}'s session compacted ${entry.compactionCount}× — memory may be degraded`;
  }
  return `${agentLabel}'s session is aging — consider a fresh session`;
}
