import assert from "node:assert/strict";
import test from "node:test";
import { openThreadForgeHubFromPullRequest } from "./ProjectThreadForgeSummaryCard.tsx";
import {
  getToolPaneSnapshot,
  resetToolPaneForTests,
} from "@/features/tool-pane/toolPaneStore.ts";
import {
  getThreadForgeHubSubject,
  resetThreadForgeHubSubject,
} from "@/features/messages/lib/threadForgeHubSubjectStore.ts";
import {
  getThreadViewMode,
  setThreadViewMode,
} from "@/features/channels/lib/threadViewModePreference.ts";

test("opening a thread PR selects its hub instead of the default simulator pane", () => {
  resetToolPaneForTests();
  resetThreadForgeHubSubject();
  setThreadViewMode("split");
  assert.equal(getToolPaneSnapshot().tab, "sim");
  try {
    openThreadForgeHubFromPullRequest({
      pullRequest: { url: "https://github.com/Nuncio-hq/crew/pull/123" },
      repositoryPath: "/workspace/crew",
      worktreePath: "/workspace/crew-pr",
      branch: "fix/example",
      channelId: "channel-one",
      rootEventId: "thread-one",
    });
    assert.deepEqual(getToolPaneSnapshot(), {
      open: true,
      tab: "pr",
      poppedOut: false,
    });
    assert.equal(getThreadViewMode(), "focus");
    assert.deepEqual(getThreadForgeHubSubject(), {
      kind: "pr",
      owner: "Nuncio-hq",
      name: "crew",
      number: 123,
      repositoryPath: "/workspace/crew",
      worktreePath: "/workspace/crew-pr",
      branch: "fix/example",
      channelId: "channel-one",
      rootEventId: "thread-one",
      source: "thread",
    });
  } finally {
    resetToolPaneForTests();
    resetThreadForgeHubSubject();
    setThreadViewMode("split");
  }
});
