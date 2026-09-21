import assert from "node:assert/strict";
import test from "node:test";

import { createRecapClient } from "./api.ts";

test("recap client uses the versioned command seam and canonical thread ids", async () => {
  const calls = [];
  const invoke = async (command, args) => {
    calls.push({ command, args });
    if (command === "get_recap_settings") return { settings: {}, runtimes: [] };
    if (command === "get_thread_recap")
      return { state: "no_recap", recap: null };
    if (command === "generate_thread_recap") return { generationId: "g-1" };
    return undefined;
  };
  const client = createRecapClient(invoke);
  await client.getSettings();
  await client.getThreadRecap({
    channelId: "channel-1",
    rootEventId: "root-1",
  });
  await client.generateThreadRecap({
    channelId: "channel-1",
    rootEventId: "root-1",
    generationId: "g-1",
  });
  await client.cancelThreadRecap({
    channelId: "channel-1",
    rootEventId: "root-1",
    generationId: "g-1",
  });
  assert.deepEqual(calls, [
    { command: "get_recap_settings", args: undefined },
    {
      command: "get_thread_recap",
      args: { channelId: "channel-1", rootEventId: "root-1" },
    },
    {
      command: "generate_thread_recap",
      args: {
        channelId: "channel-1",
        rootEventId: "root-1",
        generationId: "g-1",
      },
    },
    {
      command: "cancel_thread_recap",
      args: {
        channelId: "channel-1",
        rootEventId: "root-1",
        generationId: "g-1",
      },
    },
  ]);
});
