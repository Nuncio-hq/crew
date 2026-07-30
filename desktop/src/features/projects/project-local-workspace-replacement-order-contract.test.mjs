import assert from "node:assert/strict";
import { test } from "node:test";

import { linkProjectLocalWorkspace } from "./lib/project-local-workspace-relay.ts";

const OWNER = "a".repeat(64);
const CHANNEL_ID = "018f30b4-57c0-7f10-a3f8-9f7d8e6c5b4a";
const LOCAL_PATH = "/Users/oscar/Projects/Nuncio Crew";

function event(id, name) {
  return {
    id,
    pubkey: OWNER,
    created_at: 1_753_000_000,
    kind: 30_617,
    tags: [
      ["d", "crew"],
      ["name", name],
      ["buzz-channel", CHANNEL_ID],
    ],
    content: "",
    sig: "signature",
  };
}

test("same-second replacement reads the lexically lowest NIP-01 event id", async () => {
  const retained = event("0".repeat(64), "Relay-retained name");
  const discarded = event("f".repeat(64), "Discarded name");
  const saved = {
    ...retained,
    id: "1".repeat(64),
    created_at: retained.created_at + 1,
    tags: [...retained.tags, ["buzz-location", "local", LOCAL_PATH]],
  };
  let fetchCount = 0;
  let signedInput;

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
        return fetchCount === 1 ? [discarded, retained] : [saved];
      },
      signRelayEvent: async (input) => {
        signedInput = input;
        return saved;
      },
      publishEvent: async () => {},
    },
  );

  assert.deepEqual(
    signedInput.tags.find((tag) => tag[0] === "name"),
    ["name", "Relay-retained name"],
  );
  assert.equal(signedInput.createdAt, retained.created_at + 1);
});
