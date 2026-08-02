import 'package:buzz/features/channels/agent_activity/observer_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('ObserverFrame.fromJson', () {
    test('parses conversationId and startedAt when present', () {
      final frame = ObserverFrame.fromJson({
        'seq': 3,
        'timestamp': '2024-01-01T00:00:00Z',
        'kind': 'turn_started',
        'agentIndex': 0,
        'channelId': 'chan-1',
        'conversationId': 'conv-1',
        'sessionId': 'sess-1',
        'turnId': 'turn-1',
        'startedAt': '2024-01-01T00:00:00Z',
        'payload': null,
      });

      expect(frame.conversationId, 'conv-1');
      expect(frame.startedAt, '2024-01-01T00:00:00Z');
      expect(frame.channelId, 'chan-1');
      expect(frame.turnId, 'turn-1');
    });

    test('tolerates legacy frames without conversationId', () {
      final frame = ObserverFrame.fromJson({
        'seq': 1,
        'timestamp': '2024-01-01T00:00:00Z',
        'kind': 'acp_read',
        'channelId': 'chan-1',
        'turnId': 'turn-1',
      });

      expect(frame.conversationId, isNull);
      expect(frame.startedAt, isNull);
      expect(frame.seq, 1);
      expect(frame.kind, 'acp_read');
    });
  });
}
