import 'dart:async';

import 'package:hooks_riverpod/hooks_riverpod.dart';

import 'active_agent_turns_store.dart';
import 'observer_subscription.dart';

/// Bridges [observerRelayProvider] frames into [ActiveAgentTurnsStore].
///
/// State is a monotonic version counter — watch this provider (or the derived
/// family providers below) to rebuild when turns change.
class ActiveAgentTurnsNotifier extends Notifier<int> {
  final ActiveAgentTurnsStore _store = ActiveAgentTurnsStore();
  Timer? _pruneTimer;
  static const _pruneIntervalMs = 5000;

  ActiveAgentTurnsStore get store => _store;

  @override
  int build() {
    ref.onDispose(() {
      _pruneTimer?.cancel();
      _pruneTimer = null;
    });

    final relayState = ref.watch(observerRelayProvider);
    for (final entry in relayState.framesByAgent.entries) {
      _store.syncAgentTurnsFromEvents(entry.key, entry.value);
    }

    _pruneTimer ??= Timer.periodic(
      const Duration(milliseconds: _pruneIntervalMs),
      (_) {
        final before = _store.version;
        _store.pruneExpired();
        if (_store.version != before) {
          state = _store.version;
        }
      },
    );

    return _store.version;
  }
}

final activeAgentTurnsProvider =
    NotifierProvider<ActiveAgentTurnsNotifier, int>(
      ActiveAgentTurnsNotifier.new,
    );

/// Observer-derived working agent pubkeys for a channel.
final observerWorkingAgentPubkeysForChannelProvider = Provider.autoDispose
    .family<List<String>, String>((ref, channelId) {
      ref.watch(activeAgentTurnsProvider);
      final store = ref.read(activeAgentTurnsProvider.notifier).store;
      for (final summary in store.getActiveTurnsByChannel()) {
        if (summary.channelId == channelId) {
          return summary.agentPubkeys;
        }
      }
      return const [];
    });

/// Observer-derived working agent pubkeys for a conversation/thread.
final observerWorkingAgentPubkeysForConversationProvider = Provider.autoDispose
    .family<List<String>, String>((ref, conversationId) {
      ref.watch(activeAgentTurnsProvider);
      final store = ref.read(activeAgentTurnsProvider.notifier).store;
      return store.getActiveAgentsForConversation(conversationId);
    });

final activeTurnActivityBoundsProvider = Provider.autoDispose
    .family<
      ActiveTurnActivityBounds?,
      ({List<String> agentPubkeys, String? channelId, String? conversationId})
    >((ref, args) {
      ref.watch(activeAgentTurnsProvider);
      final store = ref.read(activeAgentTurnsProvider.notifier).store;
      return store.getActiveTurnActivityBounds(
        agentPubkeys: args.agentPubkeys,
        channelId: args.channelId,
        conversationId: args.conversationId,
      );
    });
