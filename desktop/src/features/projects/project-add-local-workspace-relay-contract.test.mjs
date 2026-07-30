import assert from "node:assert/strict";
import { test } from "node:test";

import {
  createLocalWorkspaceProject,
  ProjectLocalWorkspaceCreateError,
} from "./lib/project-add-local-workspace.ts";

const OWNER = "a".repeat(64);
const CHANNEL_ID = "018f30b4-57c0-7f10-a3f8-9f7d8e6c5b4a";
const INPUT = {
  localPath: "/Users/oscar/Projects/Nuncio Crew",
  name: "Nuncio Crew",
};

test("duplicate owner and d-tag fails before a channel is created", async () => {
  let channelCreates = 0;

  await assert.rejects(
    createLocalWorkspaceProject(INPUT, {
      createChannel: async () => {
        channelCreates += 1;
        return CHANNEL_ID;
      },
      findProject: async () => ({
        channelId: null,
        dtag: "nuncio-crew",
        localPath: null,
        owner: OWNER,
        saved: { id: "existing" },
      }),
      getOwnerPubkey: async () => OWNER,
      publishAndReadBack: async () => {
        throw new Error("must not publish");
      },
    }),
    /already have a Project/i,
  );
  assert.equal(channelCreates, 0);
});

test("relay success is returned only after the publish/read-back dependency resolves", async () => {
  const calls = [];
  const saved = { id: "relay-confirmed" };

  const result = await createLocalWorkspaceProject(INPUT, {
    createChannel: async () => {
      calls.push("channel");
      return CHANNEL_ID;
    },
    findProject: async (owner, dtag) => {
      calls.push(`find:${owner}:${dtag}`);
      return null;
    },
    getOwnerPubkey: async () => {
      calls.push("identity");
      return OWNER;
    },
    publishAndReadBack: async (input) => {
      calls.push("relay");
      assert.equal(input.owner, OWNER);
      assert.equal(input.channelId, CHANNEL_ID);
      return saved;
    },
  });

  assert.deepEqual(calls, [
    "identity",
    `find:${OWNER}:nuncio-crew`,
    "channel",
    "relay",
  ]);
  assert.deepEqual(result, {
    channelId: CHANNEL_ID,
    dtag: "nuncio-crew",
    saved,
  });
});

test("a failed relay save exposes a same-identity channel retry token", async () => {
  let channelCreates = 0;
  const dependencies = {
    createChannel: async () => {
      channelCreates += 1;
      return CHANNEL_ID;
    },
    findProject: async () => null,
    getOwnerPubkey: async () => OWNER,
    publishAndReadBack: async () => {
      throw new Error("relay rejected");
    },
  };

  let retry;
  try {
    await createLocalWorkspaceProject(INPUT, dependencies);
  } catch (error) {
    assert.ok(error instanceof ProjectLocalWorkspaceCreateError);
    retry = error.retryChannel;
  }
  assert.deepEqual(retry, {
    channelId: CHANNEL_ID,
    dtag: "nuncio-crew",
    owner: OWNER,
  });

  await assert.rejects(
    createLocalWorkspaceProject(
      { ...INPUT, retryChannel: retry },
      dependencies,
    ),
    /relay rejected/i,
  );
  assert.equal(channelCreates, 1);
});

test("a retry token from another Project identity is not reused", async () => {
  let channelCreates = 0;

  await assert.rejects(
    createLocalWorkspaceProject(
      {
        ...INPUT,
        retryChannel: {
          channelId: "old-channel",
          dtag: "another-project",
          owner: OWNER,
        },
      },
      {
        createChannel: async () => {
          channelCreates += 1;
          return CHANNEL_ID;
        },
        findProject: async () => null,
        getOwnerPubkey: async () => OWNER,
        publishAndReadBack: async () => {
          throw new Error("relay rejected");
        },
      },
    ),
    /relay rejected/i,
  );
  assert.equal(channelCreates, 1);
});

test("an ACKed event recovered by exact read-back completes a retry", async () => {
  const saved = { id: "already-on-relay" };
  let channelCreates = 0;
  let publishes = 0;

  const result = await createLocalWorkspaceProject(
    {
      ...INPUT,
      retryChannel: {
        channelId: CHANNEL_ID,
        dtag: "nuncio-crew",
        owner: OWNER,
      },
    },
    {
      createChannel: async () => {
        channelCreates += 1;
        return "new-channel";
      },
      findProject: async () => ({
        channelId: CHANNEL_ID,
        dtag: "nuncio-crew",
        localPath: INPUT.localPath,
        owner: OWNER,
        saved,
      }),
      getOwnerPubkey: async () => OWNER,
      publishAndReadBack: async () => {
        publishes += 1;
        return { id: "unexpected" };
      },
    },
  );

  assert.deepEqual(result, {
    channelId: CHANNEL_ID,
    dtag: "nuncio-crew",
    saved,
  });
  assert.equal(channelCreates, 0);
  assert.equal(publishes, 0);
});

test("a retry channel is scoped to the full owner and d-tag identity", async () => {
  let channelCreates = 0;

  await assert.rejects(
    createLocalWorkspaceProject(
      {
        ...INPUT,
        retryChannel: {
          channelId: "wrong-owner-channel",
          dtag: "nuncio-crew",
          owner: "b".repeat(64),
        },
      },
      {
        createChannel: async () => {
          channelCreates += 1;
          return CHANNEL_ID;
        },
        findProject: async () => null,
        getOwnerPubkey: async () => OWNER,
        publishAndReadBack: async () => {
          throw new Error("relay rejected");
        },
      },
    ),
    /relay rejected/i,
  );
  assert.equal(channelCreates, 1);
});
