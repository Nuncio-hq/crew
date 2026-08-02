import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../channel_management_provider.dart';
import '../channel_typing_provider.dart';
import 'active_agent_turns_provider.dart';

/// Unified "agent is working" signal for a channel.
///
/// Observer turns are primary; bot typing indicators are the fallback when the
/// observer stream is absent for that scope (matches desktop agentWorkingSignal).
final workingAgentPubkeysForChannelProvider = Provider.autoDispose
    .family<List<String>, String>((ref, channelId) {
      final observer = ref.watch(
        observerWorkingAgentPubkeysForChannelProvider(channelId),
      );
      final typingEntries = ref.watch(channelTypingProvider(channelId));
      final membersAsync = ref.watch(channelMembersProvider(channelId));
      final allMembers = membersAsync.asData?.value ?? const <ChannelMember>[];
      final botPubkeys = <String>{
        for (final m in allMembers)
          if (m.isBot) m.pubkey.toLowerCase(),
      };

      final merged = <String>{
        for (final pubkey in observer) pubkey.toLowerCase(),
      };
      for (final entry in typingEntries) {
        final key = entry.pubkey.toLowerCase();
        if (botPubkeys.contains(key)) merged.add(key);
      }
      final result = merged.toList()..sort();
      return result;
    });

/// Working agents for a conversation/thread (observer) plus thread-scoped
/// typing fallback pubkeys.
final workingAgentPubkeysForConversationProvider = Provider.autoDispose
    .family<
      List<String>,
      ({String conversationId, String channelId, String? threadHeadId})
    >((ref, args) {
      final observer = ref.watch(
        observerWorkingAgentPubkeysForConversationProvider(args.conversationId),
      );
      final typingEntries = ref.watch(channelTypingProvider(args.channelId));
      final membersAsync = ref.watch(channelMembersProvider(args.channelId));
      final allMembers = membersAsync.asData?.value ?? const <ChannelMember>[];
      final botPubkeys = <String>{
        for (final m in allMembers)
          if (m.isBot) m.pubkey.toLowerCase(),
      };

      final merged = <String>{
        for (final pubkey in observer) pubkey.toLowerCase(),
      };
      for (final entry in typingEntries) {
        if (args.threadHeadId != null &&
            entry.threadHeadId != args.threadHeadId) {
          continue;
        }
        final key = entry.pubkey.toLowerCase();
        if (botPubkeys.contains(key)) merged.add(key);
      }
      final result = merged.toList()..sort();
      return result;
    });
