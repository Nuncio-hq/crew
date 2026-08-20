import assert from "node:assert/strict";
import { test } from "node:test";

import { Relay } from "nostr-tools/relay";
import {
  finalizeEvent,
  generateSecretKey,
  getPublicKey,
} from "nostr-tools/pure";

import {
  linkProjectLocalWorkspace,
  selectCurrentProjectAnnouncement,
} from "./lib/project-local-workspace-relay.ts";
import { resolveProjectChannelAgentMessage } from "./lib/project-channel-agent-context.ts";

const LIVE_RELAY_URL = process.env.CREW_LIVE_RELAY_URL;
const CHANNEL_ID = "019b6ea7-5947-76f2-b16d-c83c441bcd61";
const FIRST_PATH = "/tmp/Nuncio Crew Đồ án";
const SECOND_PATH = "/tmp/Nuncio Crew 二";

async function connectRelay(url, secretKey) {
  const relay = new Relay(url, { enableReconnect: false });
  relay.onauth = async (template) => finalizeEvent(template, secretKey);
  await relay.connect();
  await new Promise((resolve) => setTimeout(resolve, 150));
  return relay;
}

function fetchEvents(relay, filter) {
  return new Promise((resolve, reject) => {
    const events = [];
    const timeout = setTimeout(() => {
      subscription.close("live relay read timed out");
      reject(new Error("Live relay read timed out."));
    }, 5_000);
    const subscription = relay.subscribe([filter], {
      onevent: (event) => events.push(event),
      oneose: () => {
        clearTimeout(timeout);
        subscription.close();
        resolve(events);
      },
      onclose: (reason) => {
        clearTimeout(timeout);
        if (reason && reason !== "closed by caller") {
          reject(new Error(reason));
        }
      },
    });
  });
}

function relayDependencies(relay, secretKey) {
  return {
    fetchEvents: (filter) => fetchEvents(relay, filter),
    signRelayEvent: async (event) =>
      finalizeEvent(
        {
          kind: event.kind,
          created_at: event.createdAt ?? Math.floor(Date.now() / 1_000),
          tags: event.tags,
          content: event.content,
        },
        secretKey,
      ),
    publishEvent: async (event) => {
      await relay.publish(event);
    },
  };
}

test("links and relinks a Project through a real Buzz relay", {
  skip: !LIVE_RELAY_URL,
  timeout: 20_000,
}, async () => {
  const secretKey = generateSecretKey();
  const owner = getPublicKey(secretKey);
  const dtag = `crew-live-${Date.now()}`;
  let relay = await connectRelay(LIVE_RELAY_URL, secretKey);

  try {
    const initial = finalizeEvent(
      {
        kind: 30_617,
        created_at: Math.floor(Date.now() / 1_000),
        tags: [
          ["d", dtag],
          ["name", "Crew live relay verification"],
          ["clone", "https://github.com/Nuncio-hq/crew.git"],
          ["buzz-protect", "maintainers"],
          ["auth", "transient-setup-user"],
          ["future-tag", "keep-me"],
        ],
        content: "Relay-native Project workspace verification",
      },
      secretKey,
    );
    await relay.publish(initial);

    const firstLinked = await linkProjectLocalWorkspace(
      {
        owner,
        currentPubkey: owner,
        dtag,
        channelId: CHANNEL_ID,
        localPath: FIRST_PATH,
      },
      relayDependencies(relay, secretKey),
    );

    assert.equal(firstLinked.pubkey, owner);
    assert.deepEqual(
      firstLinked.tags.find((tag) => tag[0] === "d"),
      ["d", dtag],
    );
    assert.deepEqual(
      firstLinked.tags.find((tag) => tag[0] === "clone"),
      ["clone", "https://github.com/Nuncio-hq/crew.git"],
    );
    assert.deepEqual(
      firstLinked.tags.find((tag) => tag[0] === "buzz-protect"),
      ["buzz-protect", "maintainers"],
    );
    assert.equal(
      firstLinked.tags.some((tag) => tag[0] === "auth"),
      false,
    );
    assert.ok(firstLinked.created_at >= initial.created_at + 1);
    assert.ok(
      Math.abs(firstLinked.created_at - Math.floor(Date.now() / 1_000)) <= 5,
    );

    relay.close();
    relay = await connectRelay(LIVE_RELAY_URL, secretKey);
    const afterReconnect = selectCurrentProjectAnnouncement(
      await fetchEvents(relay, {
        kinds: [30_617],
        authors: [owner],
        "#d": [dtag],
        limit: 50,
      }),
      owner,
      dtag,
    );
    assert.equal(afterReconnect?.id, firstLinked.id);

    const relinked = await linkProjectLocalWorkspace(
      {
        owner,
        currentPubkey: owner,
        dtag,
        channelId: CHANNEL_ID,
        localPath: SECOND_PATH,
      },
      relayDependencies(relay, secretKey),
    );
    assert.ok(relinked.created_at >= firstLinked.created_at + 1);
    assert.ok(
      Math.abs(relinked.created_at - Math.floor(Date.now() / 1_000)) <= 5,
    );
    assert.deepEqual(
      relinked.tags.filter(
        (tag) => tag[0] === "buzz-location" && tag[1] === "local",
      ),
      [["buzz-location", "local", SECOND_PATH]],
    );
    const agentMessage = await resolveProjectChannelAgentMessage(
      {
        channelId: CHANNEL_ID,
        content: "@codex inspect the Project",
        explicitAgentPubkeys: ["f".repeat(64)],
        ownerPubkey: owner,
      },
      {
        fetchProjectAnnouncements: (filter) => fetchEvents(relay, filter),
        fetchProjectDeletions: (filter) => fetchEvents(relay, filter),
      },
    );
    assert.match(agentMessage, /30617:/);
    assert.match(agentMessage, /Nuncio Crew 二/);
    assert.match(
      agentMessage,
      /harness provisions one isolated worktree per thread/,
    );
    assert.doesNotMatch(agentMessage, /Đồ án/);
  } finally {
    relay.close();
  }
});
