import 'package:buzz/features/channels/agent_activity/active_agent_turns_store.dart';
import 'package:buzz/features/channels/agent_activity/observer_models.dart';
import 'package:flutter_test/flutter_test.dart';

const agent =
    'abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234';

ObserverFrame makeEvent({
  int seq = 1,
  String timestamp = '2024-01-01T00:00:00Z',
  String kind = 'turn_started',
  String? channelId = 'chan-1',
  String? conversationId,
  String? turnId = 'turn-1',
  String? startedAt,
}) {
  return ObserverFrame(
    seq: seq,
    timestamp: timestamp,
    kind: kind,
    channelId: channelId,
    conversationId: conversationId,
    turnId: turnId,
    startedAt: startedAt,
  );
}

void main() {
  late ActiveAgentTurnsStore store;

  setUp(() {
    store = ActiveAgentTurnsStore();
  });

  test('tracks turn_started and clears on turn_completed', () {
    store.syncAgentTurnsFromEvents(agent, [
      makeEvent(seq: 1, conversationId: 'conv-a'),
    ]);
    expect(store.getActiveTurnsForAgent(agent), hasLength(1));
    expect(store.getActiveAgentsForConversation('conv-a'), [agent]);

    store.syncAgentTurnsFromEvents(agent, [
      makeEvent(
        seq: 2,
        timestamp: '2024-01-01T00:00:01Z',
        kind: 'turn_completed',
        conversationId: 'conv-a',
      ),
    ]);
    expect(store.getActiveTurnsForAgent(agent), isEmpty);
    expect(store.getActiveAgentsForConversation('conv-a'), isEmpty);
  });

  test('keeps exact control targets for concurrent threads in one channel', () {
    store.syncAgentTurnsFromEvents(agent, [
      makeEvent(
        seq: 1,
        channelId: 'shared-channel',
        conversationId: 'thread-a',
        turnId: 'turn-a',
      ),
      makeEvent(
        seq: 2,
        timestamp: '2024-01-01T00:00:01Z',
        channelId: 'shared-channel',
        conversationId: 'thread-b',
        turnId: 'turn-b',
      ),
    ]);

    expect(store.getActiveTurnControlTargetsForAgent(agent), [
      isA<ActiveTurnControlTarget>()
          .having((t) => t.channelId, 'channelId', 'shared-channel')
          .having((t) => t.conversationId, 'conversationId', 'thread-a')
          .having((t) => t.turnId, 'turnId', 'turn-a'),
      isA<ActiveTurnControlTarget>()
          .having((t) => t.channelId, 'channelId', 'shared-channel')
          .having((t) => t.conversationId, 'conversationId', 'thread-b')
          .having((t) => t.turnId, 'turnId', 'turn-b'),
    ]);
    expect(
      store.getActiveTurnsForAgent(agent),
      hasLength(1),
      reason: 'visual channel badge remains aggregated',
    );
  });

  test('falls back conversationId to channelId when missing', () {
    store.syncAgentTurnsFromEvents(agent, [
      makeEvent(seq: 1, channelId: 'chan-1', conversationId: null),
    ]);
    expect(store.getActiveAgentsForConversation('chan-1'), [agent]);
  });

  test('skips events at or below the watermark', () {
    store.syncAgentTurnsFromEvents(agent, [
      makeEvent(seq: 5, turnId: 't1', channelId: 'c1'),
    ]);
    store.syncAgentTurnsFromEvents(agent, [
      makeEvent(seq: 3, turnId: 't2', channelId: 'c2'),
    ]);
    final channels = store
        .getActiveTurnsForAgent(agent)
        .map((t) => t.channelId);
    expect(channels, ['c1']);
  });

  test('scopes activity bounds by conversationId', () {
    store.syncAgentTurnsFromEvents(agent, [
      makeEvent(
        seq: 1,
        channelId: 'shared',
        conversationId: 'thread-a',
        turnId: 'turn-a',
      ),
      makeEvent(
        seq: 2,
        timestamp: '2024-01-01T00:00:01Z',
        channelId: 'shared',
        conversationId: 'thread-b',
        turnId: 'turn-b',
      ),
    ]);

    final boundsA = store.getActiveTurnActivityBounds(
      agentPubkeys: [agent],
      channelId: 'shared',
      conversationId: 'thread-a',
    );
    final boundsB = store.getActiveTurnActivityBounds(
      agentPubkeys: [agent],
      channelId: 'shared',
      conversationId: 'thread-b',
    );
    expect(boundsA, isNotNull);
    expect(boundsB, isNotNull);
    expect(boundsA!.anchorAt, lessThan(boundsB!.anchorAt));
  });
}
