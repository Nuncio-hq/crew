import assert from "node:assert/strict";
import { afterEach, mock, test } from "node:test";

import { focusManager, QueryClientProvider } from "@tanstack/react-query";
import { JSDOM } from "jsdom";
import React from "react";
import { renderHook, waitFor } from "@testing-library/react";

import { useChannelMessagesQuery, useChannelSubscription } from "../hooks.ts";
import { hydrateChannelHeads } from "./channelHeadCache.ts";
import { relayClient } from "../../../shared/api/relayClient.ts";
import { createBuzzQueryClient } from "../../../shared/api/queryClient.ts";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "https://crew.test/",
});

globalThis.window = dom.window;
globalThis.document = dom.window.document;
globalThis.HTMLElement = dom.window.HTMLElement;
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
dom.window.document.hasFocus = () => true;

afterEach(() => {
  focusManager.setFocused(undefined);
});

const channel = {
  id: "channel-a",
  name: "general",
  channelType: "stream",
  visibility: "open",
  description: "",
  topic: null,
  purpose: null,
  memberCount: 1,
  memberPubkeys: [],
  lastMessageAt: null,
  archivedAt: null,
  participants: [],
  participantPubkeys: [],
  isMember: true,
  ttlSeconds: null,
  ttlDeadline: null,
};

const persistedRoot = {
  id: "a".repeat(64),
  pubkey: "b".repeat(64),
  created_at: 10,
  kind: 9,
  tags: [["h", channel.id]],
  content: "persisted",
  sig: "",
};

const replacement = {
  ...persistedRoot,
  id: "c".repeat(64),
  created_at: 11,
  content: "relay",
};

function bounds() {
  return {
    id: "d".repeat(64),
    pubkey: "e".repeat(64),
    created_at: 12,
    kind: 39006,
    tags: [
      ["h", channel.id],
      ["d", `${channel.id}:head`],
    ],
    content: JSON.stringify({ has_more: false, next_cursor: null }),
    sig: "",
  };
}

test("a hydrated active channel recovers after a concurrent reconnect and one failed window fetch", async () => {
  let windowCalls = 0;
  let reconnectListener;
  let reconnectFires = 0;

  window.localStorage.clear();
  window.__TAURI_INTERNALS__ = {
    invoke: async (command) => {
      if (command === "channel_head_cache_load") {
        await new Promise((resolve) => setTimeout(resolve, 25));
        return [
          {
            channelId: channel.id,
            events: [persistedRoot, bounds()],
          },
        ];
      }
      if (command === "get_channel_window") {
        windowCalls += 1;
        if (windowCalls === 1) {
          throw new Error("transient channel window failure");
        }
        return [replacement, bounds()];
      }
      if (command === "channel_head_cache_store") {
        return null;
      }
      return null;
    },
  };

  mock.method(relayClient, "subscribeToReconnects", (listener) => {
    reconnectListener = listener;
    return () => {};
  });
  mock.method(relayClient, "subscribeToChannelLive", async () => {
    reconnectFires += 1;
    reconnectListener?.();
    return async () => {};
  });

  const queryClient = createBuzzQueryClient();
  focusManager.setFocused(true);
  const hydration = hydrateChannelHeads(queryClient, {
    pubkey: "f".repeat(64),
    relayUrl: "wss://relay",
  });
  const wrapper = ({ children }) =>
    React.createElement(QueryClientProvider, { client: queryClient }, children);
  const view = renderHook(
    () => {
      useChannelSubscription(channel);
      return useChannelMessagesQuery(channel);
    },
    { wrapper },
  );

  try {
    await hydration;
    await waitFor(
      () => {
        assert.deepEqual(view.result.current.data, [replacement]);
        assert.equal(reconnectFires, 1);
        assert.ok(windowCalls >= 2);
      },
      { timeout: 4_000 },
    );
  } finally {
    view.unmount();
    queryClient.clear();
    mock.restoreAll();
  }
});
