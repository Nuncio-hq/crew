import assert from "node:assert/strict";
import { test } from "node:test";

import {
  channelViewContext,
  threadViewContext,
} from "./lib/crewViewContextModel.ts";

const CHANNEL_ID = "018f30b4-57c0-7f10-a3f8-9f7d8e6c5b4a";
const ROOT_ID = "f".repeat(64);

test("channel view context describes only the visible channel", () => {
  assert.deepEqual(channelViewContext(CHANNEL_ID, "general"), {
    scope: "channel",
    view: "Channel #general",
    selection: [{ id: CHANNEL_ID, kind: "channel", title: "#general" }],
  });
  assert.equal(channelViewContext(null, "general"), null);
  assert.equal(channelViewContext(CHANNEL_ID, null), null);
});

test("thread view context carries the visible thread, repository and PR", () => {
  const context = threadViewContext({
    branch: "devin/issue-272",
    channelId: CHANNEL_ID,
    channelName: "crew",
    pullRequest: { number: 272, title: "Projects agent context" },
    repositoryPath: "/Users/oscar/Projects/Nuncio Crew",
    threadRootId: ROOT_ID,
    threadTitle: "Port the selection context",
  });
  assert.equal(context.scope, "thread");
  assert.equal(context.view, "Thread in #crew");
  assert.deepEqual(context.selection, [
    { id: ROOT_ID, kind: "task", title: "Port the selection context" },
    {
      id: CHANNEL_ID,
      kind: "channel",
      title: "#crew",
    },
    {
      id: "/Users/oscar/Projects/Nuncio Crew#devin/issue-272",
      kind: "repository",
      title: "Nuncio Crew on devin/issue-272",
    },
    { id: "pr-272", kind: "review", title: "PR #272 Projects agent context" },
  ]);
});

test("thread view context omits workspace entries that are not visible yet", () => {
  const context = threadViewContext({
    branch: null,
    channelId: CHANNEL_ID,
    channelName: "crew",
    pullRequest: null,
    repositoryPath: null,
    threadRootId: ROOT_ID,
    threadTitle: "",
  });
  assert.deepEqual(context.selection, [
    { id: CHANNEL_ID, kind: "channel", title: "#crew" },
  ]);
  assert.equal(
    threadViewContext({
      branch: null,
      channelId: null,
      channelName: null,
      pullRequest: null,
      repositoryPath: null,
      threadRootId: null,
      threadTitle: "",
    }),
    null,
  );
});
