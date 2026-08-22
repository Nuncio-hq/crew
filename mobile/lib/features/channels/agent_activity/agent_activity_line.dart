import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../../shared/profile/user_cache_provider.dart';
import '../../../shared/theme/theme.dart';
import '../../../shared/widgets/buzz_loading_indicator.dart';
import '../channel_management_provider.dart';
import '../date_formatters.dart';
import '../small_avatar.dart';
import 'active_agent_turns_provider.dart';
import 'agent_activity_chrome.dart';
import 'agent_activity_sheet.dart';
import 'agent_working_signal.dart';
import 'observer_control.dart';

/// Static inline activity line above the composer (mobile M2 / desktop phase 3).
///
/// Pulse + name/count + elapsed + Stop. Hidden until [activitySilenceMs];
/// amber when stuck past [activityStuckMs].
class AgentActivityLine extends HookConsumerWidget {
  final String channelId;
  final String? conversationId;
  final String? threadHeadId;

  const AgentActivityLine({
    super.key,
    required this.channelId,
    this.conversationId,
    this.threadHeadId,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final workingPubkeys = conversationId != null
        ? ref.watch(
            workingAgentPubkeysForConversationProvider((
              conversationId: conversationId!,
              channelId: channelId,
              threadHeadId: threadHeadId,
            )),
          )
        : ref.watch(workingAgentPubkeysForChannelProvider(channelId));

    final membersAsync = ref.watch(channelMembersProvider(channelId));
    final members = membersAsync.asData?.value ?? const <ChannelMember>[];
    final memberByPubkey = <String, ChannelMember>{
      for (final m in members) m.pubkey.toLowerCase(): m,
    };

    final agents = [
      for (final pubkey in workingPubkeys)
        (
          pubkey: pubkey,
          name: memberByPubkey[pubkey]?.displayName?.trim().isNotEmpty == true
              ? memberByPubkey[pubkey]!.displayName!.trim()
              : shortPubkey(pubkey),
        ),
    ];

    final bounds = ref.watch(
      activeTurnActivityBoundsProvider((
        agentPubkeys: workingPubkeys,
        channelId: channelId,
        conversationId: conversationId,
      )),
    );

    final now = useState(DateTime.now().millisecondsSinceEpoch);
    useEffect(() {
      if (agents.isEmpty) return null;
      final timer = Timer.periodic(const Duration(seconds: 1), (_) {
        now.value = DateTime.now().millisecondsSinceEpoch;
      });
      return timer.cancel;
    }, [agents.length]);

    useEffect(() {
      if (workingPubkeys.isEmpty) return null;
      ref.read(userCacheProvider.notifier).preload(workingPubkeys);
      return null;
    }, [workingPubkeys.join(',')]);

    if (agents.isEmpty) return const SizedBox.shrink();

    final elapsedMs = bounds == null
        ? 0
        : (now.value - bounds.anchorAt).clamp(0, 1 << 62);
    if (bounds != null && elapsedMs < activitySilenceMs) {
      return const SizedBox.shrink();
    }

    final stuck =
        bounds != null && now.value - bounds.lastActivityAt >= activityStuckMs;

    final triggerLabel = agents.length == 1
        ? '${agents.first.name} ${AgentActivityChrome.isWorking}'
        : AgentActivityChrome.agentsWorking(agents.length);
    final statusLabel = stuck
        ? '$triggerLabel · ${AgentActivityChrome.seemsStuck}'
        : bounds != null
        ? '$triggerLabel · ${formatElapsed(elapsedMs)}'
        : triggerLabel;

    final stopping = useState(false);
    final userCache = ref.watch(userCacheProvider);
    final statusColor = stuck
        ? context.appColors.warning
        : context.colors.onSurfaceVariant;

    Future<void> handleStop() async {
      if (stopping.value || agents.isEmpty) return;
      stopping.value = true;
      try {
        final store = ref.read(activeAgentTurnsProvider.notifier).store;
        await Future.wait([
          for (final agent in agents)
            for (final target in store.getActiveTurnControlTargetsForAgent(
              agent.pubkey,
            ))
              if ((conversationId == null ||
                      target.conversationId == conversationId) &&
                  target.channelId == channelId)
                cancelManagedAgentTurn(
                  ref,
                  agentPubkey: agent.pubkey,
                  channelId: target.channelId,
                  conversationId: target.conversationId,
                  turnId: target.turnId,
                ),
        ]);
      } finally {
        stopping.value = false;
      }
    }

    void openActivity(String pubkey) {
      showModalBottomSheet<void>(
        context: context,
        isScrollControlled: true,
        backgroundColor: context.colors.surface,
        shape: const RoundedRectangleBorder(
          borderRadius: BorderRadius.vertical(
            top: Radius.circular(Radii.dialog),
          ),
        ),
        builder: (_) =>
            AgentActivitySheet(channelId: channelId, agentPubkey: pubkey),
      );
    }

    return Padding(
      padding: const EdgeInsets.fromLTRB(
        Grid.gutter,
        0,
        Grid.gutter,
        Grid.half,
      ),
      child: Row(
        children: [
          Expanded(
            child: InkWell(
              onTap: () {
                if (agents.length == 1) {
                  openActivity(agents.first.pubkey);
                  return;
                }
                showModalBottomSheet<void>(
                  context: context,
                  backgroundColor: context.colors.surface,
                  shape: const RoundedRectangleBorder(
                    borderRadius: BorderRadius.vertical(
                      top: Radius.circular(Radii.dialog),
                    ),
                  ),
                  builder: (sheetContext) {
                    return SafeArea(
                      child: Padding(
                        padding: const EdgeInsets.all(Grid.xs),
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              AgentActivityChrome.agentsWorkingLabel,
                              style: context.textTheme.labelMedium?.copyWith(
                                color: context.colors.onSurfaceVariant,
                              ),
                            ),
                            const SizedBox(height: Grid.xxs),
                            for (final agent in agents)
                              ListTile(
                                contentPadding: EdgeInsets.zero,
                                leading: SmallAvatar(
                                  pubkey: agent.pubkey,
                                  userCache: userCache,
                                  size: 28,
                                ),
                                title: Text(agent.name),
                                trailing: Text(
                                  AgentActivityChrome.viewActivity,
                                  style: context.textTheme.labelSmall?.copyWith(
                                    color: context.colors.onSurfaceVariant,
                                  ),
                                ),
                                onTap: () {
                                  Navigator.of(sheetContext).pop();
                                  openActivity(agent.pubkey);
                                },
                              ),
                          ],
                        ),
                      ),
                    );
                  },
                );
              },
              borderRadius: BorderRadius.circular(Radii.sm),
              child: Padding(
                padding: const EdgeInsets.symmetric(vertical: Grid.half),
                child: Row(
                  children: [
                    SizedBox(
                      width: 18,
                      height: 18,
                      child: Stack(
                        clipBehavior: Clip.none,
                        children: [
                          for (var i = 0; i < agents.take(2).length; i++)
                            Positioned(
                              left: i * 8.0,
                              child: SmallAvatar(
                                pubkey: agents[i].pubkey,
                                userCache: userCache,
                                size: 18,
                              ),
                            ),
                        ],
                      ),
                    ),
                    if (agents.length > 2) ...[
                      const SizedBox(width: Grid.xxs),
                      Text(
                        '+${agents.length - 2}',
                        style: context.textTheme.labelSmall?.copyWith(
                          color: statusColor,
                        ),
                      ),
                    ],
                    const SizedBox(width: Grid.xxs),
                    Expanded(
                      child: _PulsingLabel(
                        text: statusLabel,
                        color: statusColor,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
          TextButton(
            onPressed: stopping.value ? null : handleStop,
            style: TextButton.styleFrom(
              foregroundColor: statusColor,
              padding: const EdgeInsets.symmetric(
                horizontal: Grid.xxs,
                vertical: Grid.half,
              ),
              minimumSize: Size.zero,
              tapTargetSize: MaterialTapTargetSize.shrinkWrap,
              textStyle: context.textTheme.labelSmall?.copyWith(
                fontWeight: FontWeight.w600,
              ),
            ),
            child: stopping.value
                ? const SizedBox(
                    width: 14,
                    height: 14,
                    child: BuzzLoadingIndicator(size: 14),
                  )
                : const Text(AgentActivityChrome.stop),
          ),
        ],
      ),
    );
  }
}

class _PulsingLabel extends HookWidget {
  final String text;
  final Color color;

  const _PulsingLabel({required this.text, required this.color});

  @override
  Widget build(BuildContext context) {
    final reducedMotion = MediaQuery.disableAnimationsOf(context);
    final controller = useAnimationController(
      duration: const Duration(milliseconds: 1400),
    );
    useEffect(() {
      if (reducedMotion) {
        controller
          ..stop()
          ..value = 1;
      } else {
        controller.repeat(reverse: true);
      }
      return controller.stop;
    }, [reducedMotion]);

    final opacityAnim = useAnimation(
      Tween<double>(
        begin: 0.55,
        end: 1,
      ).animate(CurvedAnimation(parent: controller, curve: Curves.easeInOut)),
    );

    return Opacity(
      opacity: reducedMotion ? 1 : opacityAnim,
      child: Text(
        text,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: context.textTheme.labelMedium?.copyWith(color: color),
      ),
    );
  }
}
