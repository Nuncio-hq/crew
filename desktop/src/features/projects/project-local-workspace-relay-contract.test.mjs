import assert from "node:assert/strict";
import { test } from "node:test";

import {
  linkProjectLocalWorkspace,
  publishProjectAnnouncementAndReadBack,
} from "./lib/project-local-workspace-relay.ts";

const OWNER = "a".repeat(64);
const CHANNEL_ID = "018f30b4-57c0-7f10-a3f8-9f7d8e6c5b4a";
const LOCAL_PATH = "/Users/oscar/Projects/Nuncio Crew";

function relayEvent({
  id = "signed-event",
  createdAt = 1_753_000_000,
  localPath = LOCAL_PATH,
  channelId = CHANNEL_ID,
} = {}) {
  return {
    id,
    pubkey: OWNER,
    created_at: createdAt,
    kind: 30_617,
    tags: [
      ["d", "crew"],
      ["name", "Crew"],
      ["buzz-channel", channelId],
      ["buzz-location", "local", localPath],
    ],
    content: "",
    sig: "signature",
  };
}

test("publish waits for relay acknowledgement then reads back the signed event", async () => {
  const signed = relayEvent();
  const calls = [];
  const result = await publishProjectAnnouncementAndReadBack(
    {
      event: { kind: 30_617, tags: signed.tags, content: "" },
      owner: OWNER,
      dtag: "crew",
      channelId: CHANNEL_ID,
      localPath: LOCAL_PATH,
    },
    {
      signRelayEvent: async (event) => {
        calls.push(["sign", event]);
        return signed;
      },
      publishEvent: async (event) => {
        calls.push(["publish", event.id]);
      },
      fetchEvents: async (filter) => {
        calls.push(["read", filter]);
        return [signed];
      },
    },
  );

  assert.deepEqual(
    calls.map(([name]) => name),
    ["sign", "publish", "read"],
  );
  assert.deepEqual(calls[2][1], {
    ids: [signed.id],
    kinds: [30_617],
    authors: [OWNER],
    "#d": ["crew"],
    limit: 1,
  });
  assert.equal(result.id, signed.id);
});

test("a relay rejection does not fall back to optimistic local state", async () => {
  let readCount = 0;

  await assert.rejects(
    publishProjectAnnouncementAndReadBack(
      {
        event: { kind: 30_617, tags: relayEvent().tags, content: "" },
        owner: OWNER,
        dtag: "crew",
        channelId: CHANNEL_ID,
        localPath: LOCAL_PATH,
      },
      {
        signRelayEvent: async () => relayEvent(),
        publishEvent: async () => {
          throw new Error("relay rejected event");
        },
        fetchEvents: async () => {
          readCount += 1;
          return [];
        },
      },
    ),
    /relay rejected event/i,
  );

  assert.equal(readCount, 0);
});

test("missing or mismatched relay read-back is a failed save", async () => {
  for (const readBack of [
    [],
    [relayEvent({ localPath: "/Users/oscar/Projects/wrong" })],
    [relayEvent({ channelId: "wrong-channel" })],
  ]) {
    await assert.rejects(
      publishProjectAnnouncementAndReadBack(
        {
          event: { kind: 30_617, tags: relayEvent().tags, content: "" },
          owner: OWNER,
          dtag: "crew",
          channelId: CHANNEL_ID,
          localPath: LOCAL_PATH,
        },
        {
          signRelayEvent: async () => relayEvent(),
          publishEvent: async () => {},
          fetchEvents: async () => readBack,
        },
      ),
      /relay read-back/i,
    );
  }
});

test("link reads the latest Buzz event and adds its canonical channel", async () => {
  const older = relayEvent({ id: "older", createdAt: 100 });
  const current = {
    ...relayEvent({ id: "current", createdAt: 200, localPath: "/tmp/old" }),
    tags: [
      ["d", "crew"],
      ["name", "Current relay name"],
      ["clone", "https://github.com/Nuncio-hq/crew.git"],
      ["buzz-protect", "maintainers"],
      ["auth", "temporary-setup-user"],
      ["future-tag", "keep-me"],
      ["buzz-location", "local", "/tmp/old"],
    ],
  };
  let fetchCount = 0;
  let signedInput;
  const saved = {
    ...current,
    id: "saved",
    created_at: 300,
    tags: [
      ...current.tags.slice(0, -1).filter((tag) => tag[0] !== "auth"),
      ["buzz-channel", CHANNEL_ID],
      ["buzz-location", "local", LOCAL_PATH],
    ],
  };

  await linkProjectLocalWorkspace(
    {
      owner: OWNER,
      currentPubkey: OWNER,
      dtag: "crew",
      channelId: CHANNEL_ID,
      localPath: LOCAL_PATH,
    },
    {
      fetchEvents: async () => {
        fetchCount += 1;
        return fetchCount === 1 ? [older, current] : [saved];
      },
      signRelayEvent: async (event) => {
        signedInput = event;
        return saved;
      },
      publishEvent: async () => {},
    },
  );

  const after = Math.floor(Date.now() / 1_000);
  assert.deepEqual(signedInput.tags, saved.tags);
  assert.equal(signedInput.content, current.content);
  assert.ok(signedInput.createdAt >= current.created_at + 1);
  assert.ok(signedInput.createdAt >= after - 1);
  assert.ok(signedInput.createdAt <= after);
  assert.equal(
    signedInput.tags.filter(
      (tag) => tag[0] === "buzz-location" && tag[1] === "local",
    ).length,
    1,
  );
  assert.deepEqual(
    signedInput.tags.find((tag) => tag[0] === "buzz-protect"),
    ["buzz-protect", "maintainers"],
  );
  assert.equal(
    signedInput.tags.some((tag) => tag[0] === "auth"),
    false,
  );
});

test("linking refuses to sign a Project owned by another identity", async () => {
  let fetchCount = 0;

  await assert.rejects(
    linkProjectLocalWorkspace(
      {
        owner: OWNER,
        currentPubkey: "b".repeat(64),
        dtag: "crew",
        channelId: CHANNEL_ID,
        localPath: LOCAL_PATH,
      },
      {
        fetchEvents: async () => {
          fetchCount += 1;
          return [];
        },
        signRelayEvent: async () => relayEvent(),
        publishEvent: async () => {},
      },
    ),
    /own Project/i,
  );

  assert.equal(fetchCount, 0);
});

test("signed identity mismatch is rejected before publication", async () => {
  let publishCount = 0;
  const current = relayEvent({ localPath: "/tmp/old" });

  await assert.rejects(
    linkProjectLocalWorkspace(
      {
        owner: OWNER,
        currentPubkey: OWNER,
        dtag: "crew",
        channelId: CHANNEL_ID,
        localPath: LOCAL_PATH,
      },
      {
        fetchEvents: async () => [current],
        signRelayEvent: async (input) => ({
          ...relayEvent(),
          pubkey: "b".repeat(64),
          created_at: input.createdAt,
          tags: input.tags,
        }),
        publishEvent: async () => {
          publishCount += 1;
        },
      },
    ),
    /signed Project owner/i,
  );

  assert.equal(publishCount, 0);
});
