import 'package:buzz/features/channels/agent_activity/conversation_id.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('deriveAgentConversationId', () {
    test('matches Rust / desktop conversation identity vectors', () {
      expect(
        deriveAgentConversationId(
          '00112233-4455-6677-8899-aabbccddeeff',
          '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
        ),
        '7415ce56-7adc-d430-f133-c5e06a8e5113',
      );
      expect(
        deriveAgentConversationId(
          '11111111-2222-3333-4444-555555555555',
          'abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd',
        ),
        '026dfba8-bd95-7847-6709-920a0e6d9b97',
      );
    });

    test('returns null for malformed channel or root IDs', () {
      expect(
        deriveAgentConversationIdOrNull(
          'not-a-uuid',
          '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
        ),
        isNull,
      );
      expect(
        deriveAgentConversationIdOrNull(
          '00112233-4455-6677-8899-aabbccddeeff',
          'not-an-event-id',
        ),
        isNull,
      );
      expect(deriveAgentConversationIdOrNull(null, null), isNull);
    });

    test('DM surface uses channel id directly', () {
      expect(
        conversationIdForSurface(
          channelId: '00112233-4455-6677-8899-aabbccddeeff',
          isDm: true,
          rootEventId:
              '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
        ),
        '00112233-4455-6677-8899-aabbccddeeff',
      );
    });
  });
}
