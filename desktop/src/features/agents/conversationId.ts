import { sha256 } from "@noble/hashes/sha2.js";

const CONVERSATION_DOMAIN = new TextEncoder().encode(
  "buzz-acp-conversation-v1",
);

function decodeUuid(uuid: string): Uint8Array {
  const hex = uuid.replaceAll("-", "");
  if (!/^[0-9a-f]{32}$/i.test(hex)) {
    throw new Error(`Invalid UUID: ${uuid}`);
  }
  return Uint8Array.from({ length: 16 }, (_, index) =>
    Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16),
  );
}

function formatUuid(bytes: Uint8Array): string {
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0"));
  return [
    hex.slice(0, 4).join(""),
    hex.slice(4, 6).join(""),
    hex.slice(6, 8).join(""),
    hex.slice(8, 10).join(""),
    hex.slice(10, 16).join(""),
  ].join("-");
}

export function deriveAgentConversationId(
  channelId: string,
  rootEventId: string,
): string {
  if (!/^[0-9a-f]{64}$/.test(rootEventId)) {
    throw new Error(`Invalid root event ID: ${rootEventId}`);
  }
  const channelBytes = decodeUuid(channelId);
  const rootBytes = new TextEncoder().encode(rootEventId);
  const input = new Uint8Array(
    CONVERSATION_DOMAIN.length + channelBytes.length + rootBytes.length,
  );
  input.set(CONVERSATION_DOMAIN);
  input.set(channelBytes, CONVERSATION_DOMAIN.length);
  input.set(rootBytes, CONVERSATION_DOMAIN.length + channelBytes.length);
  return formatUuid(sha256(input).slice(0, 16));
}

export function deriveAgentConversationIdOrNull(
  channelId: string | null | undefined,
  rootEventId: string | null | undefined,
): string | null {
  return channelId && rootEventId
    ? deriveAgentConversationId(channelId, rootEventId)
    : null;
}
