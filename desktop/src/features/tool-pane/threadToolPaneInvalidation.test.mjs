import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { QueryClient } from "@tanstack/react-query";
import { refreshChannelsQuery } from "@/features/channels/hooks.ts";
import {
  closeToolPane,
  getToolPaneSnapshot,
  openToolPane,
  resetToolPaneForTests,
} from "./toolPaneStore.ts";

const scope = {
  relayUrl: "wss://crew.example",
  viewerPubkey: "1".repeat(64),
  channelId: "11111111-1111-1111-1111-111111111111",
  rootEventId: "a".repeat(64),
};
beforeEach(() => {
  resetToolPaneForTests();
  openToolPane("plans", scope);
  closeToolPane();
});
const options = () => ({
  queryClient: new QueryClient(),
  initialSnapshotPair: null,
  relayUrl: scope.relayUrl,
  ownerPubkey: scope.viewerPubkey,
  persistSnapshot: () => {},
});

test("confirmed authoritative channel removal clears its retained thread selection", async () => {
  await refreshChannelsQuery({
    ...options(),
    fetchChannels: async () => ({
      channels: [],
      hash: "empty",
      lastMessages: [],
    }),
  });
  openToolPane(undefined, scope);
  assert.equal(getToolPaneSnapshot().tab, "context");
});

test("failed or incomplete channel queries do not fabricate removal", async () => {
  for (const fetchChannels of [
    async () => {
      throw new Error("offline");
    },
    async () => ({ channels: null, hash: "unknown", lastMessages: [] }),
  ]) {
    await assert.rejects(refreshChannelsQuery({ ...options(), fetchChannels }));
    openToolPane(undefined, scope);
    assert.equal(getToolPaneSnapshot().tab, "plans");
    closeToolPane();
  }
});

test("channel refresh started before community reset cannot clear a new view", async () => {
  let resolve;
  const pending = refreshChannelsQuery({
    ...options(),
    fetchChannels: () =>
      new Promise((done) => {
        resolve = done;
      }),
  });
  resetToolPaneForTests();
  openToolPane("plans", scope);
  resolve({ channels: [], hash: "old", lastMessages: [] });
  await pending;
  assert.equal(getToolPaneSnapshot().open, true);
  assert.equal(getToolPaneSnapshot().tab, "plans");
});
