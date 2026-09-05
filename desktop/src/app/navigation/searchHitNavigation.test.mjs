import assert from "node:assert/strict";
import test from "node:test";

import {
  clearSearchHitEventCache,
  getCachedSearchHitEvent,
} from "./searchHitEventCache.ts";
import { openSearchHitWithNavigation } from "./searchHitNavigation.ts";

const message = {
  eventId: "message",
  content: "reply",
  kind: 9,
  pubkey: "author",
  channelId: "channel",
  channelName: "general",
  createdAt: 1,
  score: 0,
  threadRootId: "root",
};

test("search-hit navigation preserves forced message routing", async () => {
  clearSearchHitEventCache();
  const calls = [];
  const result = await openSearchHitWithNavigation(message, {
    force: true,
    goChannel: async (channelId, options) => {
      calls.push({ channelId, options });
      return true;
    },
    goForumPost: async () => false,
  });

  assert.equal(result, true);
  assert.deepEqual(calls, [
    {
      channelId: "channel",
      options: {
        force: true,
        messageId: "message",
        searchHighlight: undefined,
        threadRootId: "root",
      },
    },
  ]);
  assert.equal(getCachedSearchHitEvent("message")?.id, "message");
});

test("search-hit navigation carries trimmed highlight state and forces repeated activations", async () => {
  clearSearchHitEventCache();
  const calls = [];

  await openSearchHitWithNavigation(message, {
    goChannel: async (channelId, options) => {
      calls.push({ channelId, options });
      return true;
    },
    goForumPost: async () => false,
    query: "  Mentions  ",
  });

  assert.equal(calls[0].options.force, true);
  assert.equal(calls[0].options.searchHighlight.messageId, "message");
  assert.equal(calls[0].options.searchHighlight.query, "Mentions");
  assert.match(calls[0].options.searchHighlight.activationId, /.+/);
});

test("forum-post search navigation carries transient same-route activation state", async () => {
  clearSearchHitEventCache();
  const forumPost = { ...message, eventId: "post", kind: 45001 };
  const calls = [];

  await openSearchHitWithNavigation(forumPost, {
    goChannel: async () => false,
    goForumPost: async (channelId, postId, options) => {
      calls.push({ channelId, postId, options });
      return true;
    },
    query: "mentions",
  });

  assert.equal(calls[0].options.force, true);
  assert.equal(calls[0].options.searchHighlight.messageId, "post");
  assert.equal(calls[0].options.searchHighlight.query, "mentions");
  assert.match(calls[0].options.searchHighlight.activationId, /.+/);
});

test("cancelled search-hit navigation cannot repopulate cache or route", async () => {
  clearSearchHitEventCache();
  let resolveDestination;
  const destination = new Promise((resolve) => {
    resolveDestination = resolve;
  });
  const controller = new AbortController();
  const calls = [];
  const navigation = openSearchHitWithNavigation(
    message,
    {
      goChannel: async () => calls.push("channel"),
      goForumPost: async () => calls.push("forum"),
      signal: controller.signal,
    },
    () => destination,
  );

  controller.abort();
  resolveDestination({
    kind: "channel",
    channelId: "channel",
    messageId: "message",
    threadRootId: "root",
  });
  await navigation;

  assert.deepEqual(calls, []);
  assert.equal(getCachedSearchHitEvent("message"), null);
});

test("forum comments resolve through the channel-first forum route", async () => {
  clearSearchHitEventCache();
  const calls = [];
  await openSearchHitWithNavigation(
    {
      ...message,
      eventId: "comment",
      kind: 45003,
      threadRootId: null,
    },
    {
      force: true,
      goChannel: async () => calls.push("channel"),
      goForumPost: async (channelId, postId, options) =>
        calls.push({ channelId, postId, options }),
    },
    async () => ({
      kind: "forum-post",
      channelId: "channel",
      postId: "post",
      replyId: "comment",
    }),
  );

  assert.deepEqual(calls, [
    {
      channelId: "channel",
      postId: "post",
      options: { force: true, replyId: "comment", searchHighlight: undefined },
    },
  ]);
});
