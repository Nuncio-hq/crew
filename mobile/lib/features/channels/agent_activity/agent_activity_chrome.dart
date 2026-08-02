/// Closed set of app-authored chrome strings for agent activity UI.
/// Keep byte-identical to desktop `agentActivityChrome.ts`.
abstract final class AgentActivityChrome {
  static const isWorking = 'is working';
  static String agentsWorking(int count) =>
      count == 1 ? '1 agent working' : '$count agents working';
  static const agentsWorkingLabel = 'Agents working';
  static const viewActivity = 'View activity';
  static const stop = 'Stop';
  static const seemsStuck = 'seems stuck';
  static const workingFallback = 'Working';
}

/// Hide the live activity line until the turn has been alive this long.
const activitySilenceMs = 3000;

/// Treat a turn as stuck when no new frame arrives for this long.
const activityStuckMs = 90000;

/// Format a live elapsed duration for a ticking counter.
/// Tiers: `<60s → "Ns"` · `<60m → "Nm Ns"` · `≥60m → "Nh Nm Ns"`.
String formatElapsed(int ms) {
  final totalSeconds = ms < 0 ? 0 : ms ~/ 1000;
  if (totalSeconds < 60) return '${totalSeconds}s';
  final seconds = totalSeconds % 60;
  final totalMinutes = totalSeconds ~/ 60;
  if (totalMinutes < 60) return '${totalMinutes}m ${seconds}s';
  final minutes = totalMinutes % 60;
  final hours = totalMinutes ~/ 60;
  return '${hours}h ${minutes}m ${seconds}s';
}
