import 'observer_models.dart';

/// Harness emits turn_liveness every ~10s (BUZZ_ACP_TURN_LIVENESS_SECS).
const _livenessIntervalMs = 10000;
const _removeAfterMs = _livenessIntervalMs * 2.5;
const _frameGapPauseMs = _livenessIntervalMs * 2;
const _prunePauseMaxMs = 3 * 60 * 1000;
const _maxTurnsPerAgent = 32;
const _maxTerminalTombstones = _maxTurnsPerAgent * 4;

class ActiveTurn {
  final String turnId;
  final String channelId;
  final String conversationId;
  final int startedAt;
  int lastActivityAt;

  ActiveTurn({
    required this.turnId,
    required this.channelId,
    required this.conversationId,
    required this.startedAt,
    required this.lastActivityAt,
  });
}

class ActiveTurnSummary {
  final String channelId;
  final int anchorAt;

  const ActiveTurnSummary({required this.channelId, required this.anchorAt});
}

class ActiveTurnControlTarget {
  final String channelId;
  final String conversationId;
  final String turnId;

  const ActiveTurnControlTarget({
    required this.channelId,
    required this.conversationId,
    required this.turnId,
  });
}

class ActiveChannelTurnSummary {
  final String channelId;
  final int anchorAt;
  final int agentCount;
  final List<String> agentPubkeys;

  const ActiveChannelTurnSummary({
    required this.channelId,
    required this.anchorAt,
    required this.agentCount,
    required this.agentPubkeys,
  });
}

class ActiveConversationTurnSummary {
  final String conversationId;
  final int anchorAt;
  final int agentCount;
  final List<String> agentPubkeys;

  const ActiveConversationTurnSummary({
    required this.conversationId,
    required this.anchorAt,
    required this.agentCount,
    required this.agentPubkeys,
  });
}

class ActiveTurnActivityBounds {
  final int anchorAt;
  final int lastActivityAt;

  const ActiveTurnActivityBounds({
    required this.anchorAt,
    required this.lastActivityAt,
  });
}

/// Tracks live agent turns from observer frames. Mirrors desktop
/// `activeAgentTurnsStore.ts` (without community save/restore).
class ActiveAgentTurnsStore {
  final Map<String, Map<String, ActiveTurn>> _activeTurnsByAgent = {};
  final Map<String, int> _clockOffsetByAgent = {};
  final Map<String, ObserverFrame> _lastProcessed = {};
  final Map<String, Map<String, int>> _terminalAtByAgent = {};
  int _version = 0;

  int get version => _version;

  void syncAgentTurnsFromEvents(
    String agentPubkey,
    List<ObserverFrame> events,
  ) {
    for (final event in events) {
      _processEvent(agentPubkey, event);
    }
  }

  void clearActiveTurnsForAgent(String agentPubkey) {
    final key = agentPubkey.toLowerCase();
    final agentTurns = _activeTurnsByAgent[key];
    if (agentTurns == null || agentTurns.isEmpty) return;
    final agentClockNow =
        DateTime.now().millisecondsSinceEpoch - (_clockOffsetByAgent[key] ?? 0);
    for (final turnId in agentTurns.keys) {
      _recordTerminal(key, turnId, agentClockNow);
    }
    _activeTurnsByAgent.remove(key);
    _bump();
  }

  void reset() {
    _activeTurnsByAgent.clear();
    _lastProcessed.clear();
    _clockOffsetByAgent.clear();
    _terminalAtByAgent.clear();
    _bump();
  }

  /// Drop turns whose hosts went silent. Call from a timer.
  void pruneExpired() {
    final now = DateTime.now().millisecondsSinceEpoch;
    var changed = false;
    final agents = _activeTurnsByAgent.keys.toList();
    for (final agentKey in agents) {
      final agentTurns = _activeTurnsByAgent[agentKey]!;
      if (_shouldPausePrune(agentTurns, now)) continue;
      final turnIds = agentTurns.keys.toList();
      for (final turnId in turnIds) {
        final turn = agentTurns[turnId]!;
        if (now - turn.lastActivityAt > _removeAfterMs) {
          agentTurns.remove(turnId);
          changed = true;
        }
      }
      if (agentTurns.isEmpty) {
        _activeTurnsByAgent.remove(agentKey);
        changed = true;
      }
    }
    if (changed) _bump();
  }

  List<ActiveTurnSummary> getActiveTurnsForAgent(String? agentPubkey) {
    if (agentPubkey == null || agentPubkey.isEmpty) return const [];
    final key = agentPubkey.toLowerCase();
    final agentTurns = _activeTurnsByAgent[key];
    if (agentTurns == null || agentTurns.isEmpty) return const [];
    final offset = _clockOffsetByAgent[key] ?? 0;
    final earliestByChannel = <String, int>{};
    for (final turn in agentTurns.values) {
      final prior = earliestByChannel[turn.channelId];
      if (prior == null || turn.startedAt < prior) {
        earliestByChannel[turn.channelId] = turn.startedAt;
      }
    }
    final result = [
      for (final entry in earliestByChannel.entries)
        ActiveTurnSummary(channelId: entry.key, anchorAt: entry.value + offset),
    ]..sort((a, b) => a.channelId.compareTo(b.channelId));
    return result;
  }

  List<ActiveTurnControlTarget> getActiveTurnControlTargetsForAgent(
    String? agentPubkey,
  ) {
    if (agentPubkey == null || agentPubkey.isEmpty) return const [];
    final key = agentPubkey.toLowerCase();
    final agentTurns = _activeTurnsByAgent[key];
    if (agentTurns == null || agentTurns.isEmpty) return const [];
    final result =
        [
          for (final turn in agentTurns.values)
            ActiveTurnControlTarget(
              channelId: turn.channelId,
              conversationId: turn.conversationId,
              turnId: turn.turnId,
            ),
        ]..sort((a, b) {
          final byChannel = a.channelId.compareTo(b.channelId);
          if (byChannel != 0) return byChannel;
          final byConversation = a.conversationId.compareTo(b.conversationId);
          if (byConversation != 0) return byConversation;
          return a.turnId.compareTo(b.turnId);
        });
    return result;
  }

  List<String> getActiveAgentsForConversation(String? conversationId) {
    if (conversationId == null || conversationId.isEmpty) return const [];
    final agentPubkeys = <String>[];
    for (final entry in _activeTurnsByAgent.entries) {
      if (entry.value.values.any((t) => t.conversationId == conversationId)) {
        agentPubkeys.add(entry.key);
      }
    }
    agentPubkeys.sort();
    return agentPubkeys;
  }

  List<ActiveChannelTurnSummary> getActiveTurnsByChannel() {
    if (_activeTurnsByAgent.isEmpty) return const [];
    final summaries = <String, ({int anchorAt, Set<String> agentPubkeys})>{};
    for (final entry in _activeTurnsByAgent.entries) {
      final agentKey = entry.key;
      final agentTurns = entry.value;
      if (agentTurns.isEmpty) continue;
      final offset = _clockOffsetByAgent[agentKey] ?? 0;
      for (final turn in agentTurns.values) {
        final anchorAt = turn.startedAt + offset;
        final existing = summaries[turn.channelId];
        if (existing == null) {
          summaries[turn.channelId] = (
            anchorAt: anchorAt,
            agentPubkeys: {agentKey},
          );
        } else {
          existing.agentPubkeys.add(agentKey);
          if (anchorAt < existing.anchorAt) {
            summaries[turn.channelId] = (
              anchorAt: anchorAt,
              agentPubkeys: existing.agentPubkeys,
            );
          }
        }
      }
    }
    final result = [
      for (final entry in summaries.entries)
        ActiveChannelTurnSummary(
          channelId: entry.key,
          anchorAt: entry.value.anchorAt,
          agentCount: entry.value.agentPubkeys.length,
          agentPubkeys: entry.value.agentPubkeys.toList()..sort(),
        ),
    ]..sort((a, b) => a.channelId.compareTo(b.channelId));
    return result;
  }

  List<ActiveConversationTurnSummary> getActiveTurnsByConversation() {
    if (_activeTurnsByAgent.isEmpty) return const [];
    final summaries = <String, ({int anchorAt, Set<String> agentPubkeys})>{};
    for (final entry in _activeTurnsByAgent.entries) {
      final agentKey = entry.key;
      final agentTurns = entry.value;
      if (agentTurns.isEmpty) continue;
      final offset = _clockOffsetByAgent[agentKey] ?? 0;
      for (final turn in agentTurns.values) {
        final conversationId = turn.conversationId;
        final anchorAt = turn.startedAt + offset;
        final existing = summaries[conversationId];
        if (existing == null) {
          summaries[conversationId] = (
            anchorAt: anchorAt,
            agentPubkeys: {agentKey},
          );
        } else {
          existing.agentPubkeys.add(agentKey);
          if (anchorAt < existing.anchorAt) {
            summaries[conversationId] = (
              anchorAt: anchorAt,
              agentPubkeys: existing.agentPubkeys,
            );
          }
        }
      }
    }
    final result = [
      for (final entry in summaries.entries)
        ActiveConversationTurnSummary(
          conversationId: entry.key,
          anchorAt: entry.value.anchorAt,
          agentCount: entry.value.agentPubkeys.length,
          agentPubkeys: entry.value.agentPubkeys.toList()..sort(),
        ),
    ]..sort((a, b) => a.conversationId.compareTo(b.conversationId));
    return result;
  }

  ActiveTurnActivityBounds? getActiveTurnActivityBounds({
    required Iterable<String> agentPubkeys,
    String? channelId,
    String? conversationId,
  }) {
    final scopedChannel = channelId?.trim();
    final scopedConversation = conversationId?.trim();
    int? anchorAt;
    var lastActivityAt = 0;

    for (final pubkey in agentPubkeys) {
      final key = pubkey.toLowerCase();
      final agentTurns = _activeTurnsByAgent[key];
      if (agentTurns == null || agentTurns.isEmpty) continue;
      final offset = _clockOffsetByAgent[key] ?? 0;
      for (final turn in agentTurns.values) {
        if (scopedChannel != null &&
            scopedChannel.isNotEmpty &&
            turn.channelId != scopedChannel) {
          continue;
        }
        if (scopedConversation != null &&
            scopedConversation.isNotEmpty &&
            turn.conversationId != scopedConversation) {
          continue;
        }
        final turnAnchor = turn.startedAt + offset;
        if (anchorAt == null || turnAnchor < anchorAt) {
          anchorAt = turnAnchor;
        }
        if (turn.lastActivityAt > lastActivityAt) {
          lastActivityAt = turn.lastActivityAt;
        }
      }
    }

    if (anchorAt == null || lastActivityAt == 0) {
      return null;
    }
    return ActiveTurnActivityBounds(
      anchorAt: anchorAt,
      lastActivityAt: lastActivityAt,
    );
  }

  void _processEvent(String agentPubkey, ObserverFrame event) {
    final key = agentPubkey.toLowerCase();
    final last = _lastProcessed[key];
    if (last != null && _compareObserverFrames(event, last) <= 0) {
      return;
    }
    _lastProcessed[key] = event;

    final offsetChanged = _sampleClockOffset(key, event.timestamp);

    switch (event.kind) {
      case 'turn_started':
        if (event.channelId != null) {
          _startTurn(
            agentPubkey,
            event.channelId!,
            event.conversationId ?? event.channelId!,
            event.turnId ?? 'seq-${event.seq}',
            event.timestamp,
          );
          _bump();
          return;
        }
      case 'turn_completed':
      case 'turn_error':
      case 'agent_panic':
        _endTurn(
          agentPubkey,
          event.turnId,
          event.channelId,
          _parseTimestamp(event.timestamp) ??
              DateTime.now().millisecondsSinceEpoch,
        );
        _bump();
        return;
      case 'acp_read':
      case 'acp_write':
      case 'turn_liveness':
        final refreshed = _recordActivity(agentPubkey, event.turnId);
        if (!refreshed && _resurrectTurn(agentPubkey, event)) {
          _bump();
          return;
        }
    }

    if (offsetChanged) _bump();
  }

  bool _sampleClockOffset(String agentKey, String timestamp) {
    final parsed = _parseTimestamp(timestamp);
    if (parsed == null) return false;
    final sample = DateTime.now().millisecondsSinceEpoch - parsed;
    final prior = _clockOffsetByAgent[agentKey];
    if (prior != null && sample >= prior) return false;
    _clockOffsetByAgent[agentKey] = sample;
    return true;
  }

  void _startTurn(
    String agentPubkey,
    String channelId,
    String conversationId,
    String turnId,
    String timestamp,
  ) {
    final key = agentPubkey.toLowerCase();
    final agentTurns = _activeTurnsByAgent.putIfAbsent(
      key,
      () => <String, ActiveTurn>{},
    );

    if (agentTurns.length >= _maxTurnsPerAgent &&
        !agentTurns.containsKey(turnId)) {
      String? oldestKey;
      int? oldestTime;
      for (final entry in agentTurns.entries) {
        if (oldestTime == null || entry.value.startedAt < oldestTime) {
          oldestTime = entry.value.startedAt;
          oldestKey = entry.key;
        }
      }
      if (oldestKey != null) agentTurns.remove(oldestKey);
    }

    final startedAt =
        _parseTimestamp(timestamp) ?? DateTime.now().millisecondsSinceEpoch;
    agentTurns[turnId] = ActiveTurn(
      turnId: turnId,
      channelId: channelId,
      conversationId: conversationId,
      startedAt: startedAt,
      lastActivityAt: DateTime.now().millisecondsSinceEpoch,
    );
  }

  bool _recordActivity(String agentPubkey, String? turnId) {
    if (turnId == null) return false;
    final key = agentPubkey.toLowerCase();
    final turn = _activeTurnsByAgent[key]?[turnId];
    if (turn == null) return false;
    turn.lastActivityAt = DateTime.now().millisecondsSinceEpoch;
    return true;
  }

  bool _resurrectTurn(String agentPubkey, ObserverFrame event) {
    if (event.turnId == null || event.channelId == null) return false;
    final key = agentPubkey.toLowerCase();
    final terminalAt = _terminalAtByAgent[key]?[event.turnId!];
    final frameAt = _parseTimestamp(event.timestamp);
    if (terminalAt != null && (frameAt == null || frameAt <= terminalAt)) {
      return false;
    }
    final startedAtCandidate = event.startedAt;
    final startedAtMs = startedAtCandidate == null
        ? null
        : _parseTimestamp(startedAtCandidate);
    final safeStartedAt =
        frameAt != null &&
            startedAtMs != null &&
            startedAtMs <= frameAt &&
            startedAtCandidate != null
        ? startedAtCandidate
        : event.timestamp;
    _startTurn(
      agentPubkey,
      event.channelId!,
      event.conversationId ?? event.channelId!,
      event.turnId!,
      safeStartedAt,
    );
    return true;
  }

  void _recordTerminal(String agentKey, String turnId, int terminalAt) {
    final terminals = _terminalAtByAgent.putIfAbsent(
      agentKey,
      () => <String, int>{},
    );
    terminals[turnId] = terminalAt;
    if (terminals.length > _maxTerminalTombstones) {
      final oldest = terminals.keys.first;
      terminals.remove(oldest);
    }
  }

  void _endTurn(
    String agentPubkey,
    String? turnId,
    String? channelId,
    int terminalAt,
  ) {
    final key = agentPubkey.toLowerCase();
    if (turnId != null) {
      _recordTerminal(key, turnId, terminalAt);
    }
    final agentTurns = _activeTurnsByAgent[key];
    if (agentTurns == null) return;

    if (turnId != null) {
      agentTurns.remove(turnId);
    } else if (channelId != null) {
      final match = agentTurns.entries
          .where((e) => e.value.channelId == channelId)
          .map((e) => e.key)
          .firstOrNull;
      if (match != null) {
        agentTurns.remove(match);
        _recordTerminal(key, match, terminalAt);
      }
    }
    if (agentTurns.isEmpty) {
      _activeTurnsByAgent.remove(key);
    }
  }

  bool _shouldPausePrune(Map<String, ActiveTurn> agentTurns, int now) {
    var maxActivity = 0;
    for (final turn in agentTurns.values) {
      if (turn.lastActivityAt > maxActivity) maxActivity = turn.lastActivityAt;
    }
    final silentFor = now - maxActivity;
    return maxActivity > 0 &&
        silentFor > _frameGapPauseMs &&
        silentFor < _prunePauseMaxMs;
  }

  void _bump() => _version++;

  static int? _parseTimestamp(String timestamp) {
    final parsed = DateTime.tryParse(timestamp);
    return parsed?.millisecondsSinceEpoch;
  }

  static int _compareObserverFrames(ObserverFrame a, ObserverFrame b) {
    final tsA = _parseTimestamp(a.timestamp) ?? 0;
    final tsB = _parseTimestamp(b.timestamp) ?? 0;
    if (tsA != tsB) return tsA.compareTo(tsB);
    return a.seq.compareTo(b.seq);
  }
}
