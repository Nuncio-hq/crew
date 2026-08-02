import 'dart:convert';
import 'dart:typed_data';

import 'package:pointycastle/digests/sha256.dart';

const _conversationDomain = 'buzz-acp-conversation-v1';

/// Byte-identical to desktop `deriveAgentConversationId` / Rust `deterministic_id`.
///
/// `sha256("buzz-acp-conversation-v1" + channelUuidRaw16 + rootEventIdAsciiHex)[0..16]`
/// formatted as a UUID. Channel id is raw UUID bytes; root event id is ASCII hex.
String deriveAgentConversationId(String channelId, String rootEventId) {
  if (!RegExp(r'^[0-9a-f]{64}$').hasMatch(rootEventId)) {
    throw ArgumentError.value(
      rootEventId,
      'rootEventId',
      'Invalid root event ID',
    );
  }
  final channelBytes = _decodeUuid(channelId);
  final rootBytes = utf8.encode(rootEventId);
  final domainBytes = utf8.encode(_conversationDomain);
  final input = Uint8List(
    domainBytes.length + channelBytes.length + rootBytes.length,
  );
  input.setAll(0, domainBytes);
  input.setAll(domainBytes.length, channelBytes);
  input.setAll(domainBytes.length + channelBytes.length, rootBytes);
  final digest = SHA256Digest().process(input);
  return _formatUuid(Uint8List.sublistView(digest, 0, 16));
}

/// Same as [deriveAgentConversationId], returning null on malformed input.
String? deriveAgentConversationIdOrNull(
  String? channelId,
  String? rootEventId,
) {
  if (channelId == null ||
      channelId.isEmpty ||
      rootEventId == null ||
      rootEventId.isEmpty) {
    return null;
  }
  try {
    return deriveAgentConversationId(channelId, rootEventId);
  } catch (_) {
    return null;
  }
}

/// Conversation identity for a rendered surface.
///
/// DMs use the channel id directly (Rust `id_for_event` when `is_dm`).
/// Channel threads hash `(channelId, rootEventId)`.
String? conversationIdForSurface({
  required String channelId,
  required bool isDm,
  String? rootEventId,
}) {
  if (isDm) return channelId;
  return deriveAgentConversationIdOrNull(channelId, rootEventId);
}

Uint8List _decodeUuid(String uuid) {
  final hex = uuid.replaceAll('-', '');
  if (!RegExp(r'^[0-9a-fA-F]{32}$').hasMatch(hex)) {
    throw ArgumentError.value(uuid, 'channelId', 'Invalid UUID');
  }
  return Uint8List.fromList([
    for (var i = 0; i < 16; i++)
      int.parse(hex.substring(i * 2, i * 2 + 2), radix: 16),
  ]);
}

String _formatUuid(Uint8List bytes) {
  final hex = [
    for (final byte in bytes) byte.toRadixString(16).padLeft(2, '0'),
  ];
  return [
    hex.sublist(0, 4).join(),
    hex.sublist(4, 6).join(),
    hex.sublist(6, 8).join(),
    hex.sublist(8, 10).join(),
    hex.sublist(10, 16).join(),
  ].join('-');
}
