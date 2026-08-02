import 'package:hooks_riverpod/hooks_riverpod.dart';

import 'agent_working_signal.dart';

/// Derived provider that computes which bot members in a channel are currently
/// working. Observer turns are primary; typing is the fallback.
///
/// Used by both the members button badge and the members sheet to avoid
/// duplicating the bot-working cross-reference logic.
final workingBotPubkeysProvider = Provider.autoDispose
    .family<Set<String>, String>((ref, channelId) {
      return ref
          .watch(workingAgentPubkeysForChannelProvider(channelId))
          .toSet();
    });
