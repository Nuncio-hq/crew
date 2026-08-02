import 'dart:convert';

import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:nostr/nostr.dart' as nostr;

import '../../../shared/crypto/nip44.dart';
import '../../../shared/relay/relay.dart';

/// Send an owner→agent observer control frame (kind 24200, `frame=control`).
Future<void> sendAgentObserverControl(
  WidgetRef ref, {
  required String agentPubkey,
  required Map<String, dynamic> payload,
}) async {
  final config = ref.read(relayConfigProvider);
  final nsec = config.nsec;
  if (nsec == null || nsec.isEmpty) {
    throw StateError('Cannot send agent control: no signing key');
  }

  final privHex = nostr.Nip19.decode(payload: nsec).data;
  if (privHex.isEmpty) {
    throw StateError('Cannot send agent control: invalid nsec');
  }

  final normalizedAgent = agentPubkey.toLowerCase();
  final conversationKey = getConversationKey(privHex, normalizedAgent);
  final encrypted = nip44Encrypt(conversationKey, jsonEncode(payload));

  final relay = SignedEventRelay(
    session: ref.read(relaySessionProvider.notifier),
    nsec: nsec,
  );

  await relay.submit(
    kind: EventKind.agentObserverFrame,
    content: encrypted,
    tags: [
      ['p', normalizedAgent],
      ['agent', normalizedAgent],
      ['frame', 'control'],
    ],
  );
}

/// Cancel one live agent turn via observer control.
Future<void> cancelManagedAgentTurn(
  WidgetRef ref, {
  required String agentPubkey,
  required String channelId,
  required String conversationId,
  required String turnId,
}) {
  return sendAgentObserverControl(
    ref,
    agentPubkey: agentPubkey,
    payload: {
      'type': 'cancel_turn',
      'channelId': channelId,
      'conversationId': conversationId,
      'turnId': turnId,
    },
  );
}
