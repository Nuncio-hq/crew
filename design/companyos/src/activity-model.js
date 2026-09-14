// Authored observer examples, not real telemetry. Shared by cards and Activity.
// Production: observerRelayStore -> scoped transcript projection; no LLM call.
const samples = {
  workspace: [
    {
      at: 0,
      end: 6,
      id: "w-thought",
      agent: "Codex",
      kind: "thought",
      text: "Checking which navigation state belongs to the selected channel.",
    },
    {
      at: 6,
      end: 12,
      id: "w-read",
      agent: "Codex",
      kind: "tool",
      text: "Read · sidebar.tsx",
      result: "Read completed",
    },
    {
      at: 12,
      end: 20,
      id: "w-edit",
      agent: "Codex",
      kind: "tool",
      text: "Edit · channel navigation",
      result: "Edit completed",
    },
    {
      at: 17,
      end: 24,
      id: "w-review",
      agent: "Claude",
      kind: "tool",
      text: "Read · navigation diff",
      result: "Read completed",
    },
  ],
  ci: [
    {
      at: 0,
      end: 0,
      id: "ci-failed",
      agent: "Codex",
      kind: "tool",
      text: "Test · reconnect recovery",
      result: "Failed · expected latest reply to remain visible",
      failed: true,
    },
  ],
  remote: [
    {
      at: 0,
      end: 0,
      id: "remote-read",
      agent: "Hermes",
      kind: "tool",
      text: "Read · preview verification log",
      result: "Last received before disconnect",
    },
  ],
  "customer-followup": [
    {
      at: 0,
      end: 10,
      id: "feedback",
      agent: "Claude",
      kind: "tool",
      text: "Read · pilot feedback notes",
      result: "Read completed",
    },
  ],
  launch: [
    {
      at: 12,
      end: 18,
      id: "launch-msg",
      agent: "Hermes",
      kind: "message",
      text: "Claude, prepare two positioning options. Codex, check the audience and tracking dependency.",
    },
    {
      at: 19,
      end: 25,
      id: "audience-read",
      agent: "Codex",
      kind: "tool",
      text: "Read · audience research",
      result: "Read completed",
    },
    {
      at: 34,
      end: 42,
      id: "copy-write",
      agent: "Claude",
      kind: "tool",
      text: "Write · launch-pack.md",
      result: "Launch pack saved",
    },
    {
      at: 44,
      end: 49,
      id: "tracking-msg",
      agent: "Codex",
      kind: "message",
      text: "Signup conversion tracking is missing. I’m handing that dependency to engineering.",
    },
    {
      at: 104,
      end: 114,
      id: "schedule-write",
      agent: "Hermes",
      kind: "tool",
      text: "Write · campaign schedule draft",
      result: "Schedule draft saved",
    },
  ],
  tracking: [
    {
      at: 54,
      end: 59,
      id: "track-read",
      agent: "Codex",
      kind: "tool",
      text: "Read · signup handler",
      result: "Read completed",
    },
    {
      at: 59,
      end: 64,
      id: "track-edit",
      agent: "Codex",
      kind: "tool",
      text: "Edit · signup tracking",
      result: "Edit completed",
    },
    {
      at: 64,
      end: 68,
      id: "track-test",
      agent: "Codex",
      kind: "tool",
      text: "Test · signup conversion",
      result: "Failed · duplicate signup event",
      failed: true,
    },
    {
      at: 69,
      end: 73,
      id: "track-fix",
      agent: "Codex",
      kind: "tool",
      text: "Edit · duplicate event guard",
      result: "Edit completed",
    },
    {
      at: 73,
      end: 76,
      id: "track-retest",
      agent: "Codex",
      kind: "tool",
      text: "Test · signup conversion",
      result: "Passed",
    },
  ],
};
export function activityAt(thread, time, film = false) {
  const rows = samples[thread.id] || [];
  const now =
    film || !["launch", "tracking"].includes(thread.id)
      ? time
      : time + (thread.id === "launch" ? 12 : 54);
  return rows
    .filter((r) => r.at <= now)
    .map((r) => {
      const complete = now >= r.end;
      const progress =
        r.end === r.at ? 1 : Math.min(1, (now - r.at) / (r.end - r.at));
      return {
        ...r,
        text:
          r.kind === "tool"
            ? r.text
            : r.text.slice(
                0,
                Math.max(1, Math.floor(r.text.length * progress)),
              ),
        status: complete ? (r.failed ? "failed" : "completed") : "running",
        streaming: !complete && r.kind !== "tool",
      };
    });
}
