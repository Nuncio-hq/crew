import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

import {
  projectWorkspaceUiReadiness,
  reusableProjectWorkspaceChannel,
} from "./lib/project-local-workspace-ui.ts";

test("folder selection waits for a relay-confirmed Project announcement", () => {
  for (const announcementStatus of ["loading", "error", "missing"]) {
    assert.deepEqual(
      projectWorkspaceUiReadiness({
        announcementStatus,
        relayUrl: "ws://127.0.0.1:3000",
      }),
      { canChooseFolder: false, canPublish: false },
    );
  }
});

test("publication waits for the exact relay destination", () => {
  for (const relayUrl of [null, "", "   "]) {
    assert.deepEqual(
      projectWorkspaceUiReadiness({
        announcementStatus: "ready",
        relayUrl,
      }),
      { canChooseFolder: true, canPublish: false },
    );
  }
  assert.deepEqual(
    projectWorkspaceUiReadiness({
      announcementStatus: "ready",
      relayUrl: "ws://127.0.0.1:3000",
    }),
    { canChooseFolder: true, canPublish: true },
  );
});

test("a channel created by a failed publish is reused only for that Project", () => {
  const retry = { projectId: "project-a", channelId: "channel-a" };

  assert.equal(
    reusableProjectWorkspaceChannel("project-a", null, retry),
    "channel-a",
  );
  assert.equal(reusableProjectWorkspaceChannel("project-b", null, retry), null);
  assert.equal(
    reusableProjectWorkspaceChannel("project-a", "canonical", retry),
    "canonical",
  );
});

test("mounted folder linking does not create a channel before the durable journal", async () => {
  const source = await readFile(
    new URL("../channels/ui/ChannelLocalWorkspaceChip.tsx", import.meta.url),
    "utf8",
  );
  assert.doesNotMatch(source, /createProjectWorkspaceChannel/);
  assert.doesNotMatch(source, /reusableProjectWorkspaceChannel/);
  assert.match(source, /linkCurrentProjectWorkspace/);
});
