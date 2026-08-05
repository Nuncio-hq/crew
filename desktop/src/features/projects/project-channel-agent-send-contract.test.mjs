import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

import { resolveProjectChannelAgentMessage } from "./lib/project-channel-agent-context.ts";

const OWNER = "a".repeat(64);
const CHANNEL_ID = "018f30b4-57c0-7f10-a3f8-9f7d8e6c5b4a";
const LOCAL_PATH = "/Users/oscar/Projects/Nuncio Crew";
const AGENT = "b".repeat(64);

function announcement() {
  return {
    id: "event-1",
    pubkey: OWNER,
    created_at: 1_753_000_000,
    kind: 30_617,
    tags: [
      ["d", "crew"],
      ["buzz-channel", CHANNEL_ID],
      ["buzz-location", "local", LOCAL_PATH],
    ],
    content: "",
    sig: "signature",
  };
}

test("ordinary sends do not query Project state or change content", async () => {
  let fetchCount = 0;
  const content = await resolveProjectChannelAgentMessage(
    {
      channelId: CHANNEL_ID,
      content: "Hello team.",
      explicitAgentPubkeys: [],
      ownerPubkey: OWNER,
    },
    {
      fetchProjectAnnouncements: async () => {
        fetchCount += 1;
        return [announcement()];
      },
      fetchProjectDeletions: async () => [],
    },
  );

  assert.equal(content, "Hello team.");
  assert.equal(fetchCount, 0);
});

test("an explicit agent mention resolves fresh relay Project context", async () => {
  const content = await resolveProjectChannelAgentMessage(
    {
      channelId: CHANNEL_ID,
      content: "@codex inspect the tests.",
      explicitAgentPubkeys: [AGENT],
      ownerPubkey: OWNER,
    },
    {
      fetchProjectAnnouncements: async () => [announcement()],
      fetchProjectDeletions: async () => [],
    },
  );

  assert.match(content, /@codex inspect the tests\./);
  assert.match(content, /30617:/);
  assert.match(content, /\/Users\/oscar\/Projects\/Nuncio Crew/);
});

test("agent sends outside a Project channel remain unchanged", async () => {
  const content = await resolveProjectChannelAgentMessage(
    {
      channelId: "another-channel",
      content: "@codex say hello.",
      explicitAgentPubkeys: [AGENT],
      ownerPubkey: OWNER,
    },
    {
      fetchProjectAnnouncements: async () => [announcement()],
      fetchProjectDeletions: async () => [],
    },
  );

  assert.equal(content, "@codex say hello.");
});

test("relay failure blocks a Project-channel agent send without stale fallback", async () => {
  await assert.rejects(
    resolveProjectChannelAgentMessage(
      {
        channelId: CHANNEL_ID,
        content: "@codex inspect the tests.",
        explicitAgentPubkeys: [AGENT],
        ownerPubkey: OWNER,
      },
      {
        fetchProjectAnnouncements: async () => {
          throw new Error("relay unavailable");
        },
        fetchProjectDeletions: async () => [],
      },
    ),
    /relay unavailable/i,
  );
});

test("invalid Project workspace metadata blocks an explicit-agent send", async () => {
  const invalid = {
    ...announcement(),
    tags: [
      ["d", "crew"],
      ["buzz-channel", CHANNEL_ID],
      ["buzz-location", "local", "relative/path"],
    ],
  };

  await assert.rejects(
    resolveProjectChannelAgentMessage(
      {
        channelId: CHANNEL_ID,
        content: "@codex inspect the tests.",
        explicitAgentPubkeys: [AGENT],
        ownerPubkey: OWNER,
      },
      {
        fetchProjectAnnouncements: async () => [invalid],
        fetchProjectDeletions: async () => [],
      },
    ),
    /invalid local workspace/i,
  );
});

test("the send hook returns before clearing a draft when context resolution fails", async () => {
  // Resolve/clear ordering lives in the Crew-extracted complete-send helper
  // (D-022); the flow hook only wires it in.
  const source = await readFile(
    new URL("../messages/ui/useMentionSendComplete.ts", import.meta.url),
    "utf8",
  );
  const resolverIndex = source.indexOf(
    "await resolveCurrentProjectChannelAgentMessage",
  );
  const failureMessageIndex = source.indexOf(
    "Could not resolve Project workspace",
  );
  const failureThrowIndex = source.indexOf("throw error", failureMessageIndex);
  // No-upload path awaits finishSend (which resolves) before clearComposer.
  const finishBeforeClear = source.indexOf(
    "await finishSend([]);",
    failureThrowIndex,
  );
  const clearComposerIndex = source.indexOf(
    "clearComposer(",
    finishBeforeClear,
  );

  assert.ok(resolverIndex >= 0);
  assert.ok(failureMessageIndex > resolverIndex);
  assert.ok(failureThrowIndex > failureMessageIndex);
  assert.ok(finishBeforeClear > failureThrowIndex);
  assert.ok(clearComposerIndex > finishBeforeClear);
});

test("fresh fetch is scoped to the current identity and ignores other owners", async () => {
  const attacker = {
    ...announcement(),
    id: "attacker-event",
    pubkey: "c".repeat(64),
    tags: [
      ["d", "spoof"],
      ["buzz-channel", CHANNEL_ID],
      ["buzz-location", "local", "/tmp/attacker"],
    ],
  };
  const filters = [];
  const content = await resolveProjectChannelAgentMessage(
    {
      channelId: CHANNEL_ID,
      content: "@codex inspect.",
      explicitAgentPubkeys: [AGENT],
      ownerPubkey: OWNER,
    },
    {
      fetchProjectAnnouncements: async (filter) => {
        filters.push(filter);
        return [attacker, announcement()];
      },
      fetchProjectDeletions: async (filter) => {
        filters.push(filter);
        return [];
      },
    },
  );

  assert.deepEqual(filters[0], {
    kinds: [30_617],
    authors: [OWNER],
    limit: 2_000,
  });
  assert.deepEqual(filters[1], {
    kinds: [5],
    authors: [OWNER],
    limit: 2_000,
  });
  assert.doesNotMatch(content, /attacker/);
  assert.match(content, /Nuncio Crew/);
});

test("a deleted Project announcement does not supply agent context", async () => {
  const content = await resolveProjectChannelAgentMessage(
    {
      channelId: CHANNEL_ID,
      content: "@codex inspect.",
      explicitAgentPubkeys: [AGENT],
      ownerPubkey: OWNER,
    },
    {
      fetchProjectAnnouncements: async () => [announcement()],
      fetchProjectDeletions: async () => [
        {
          id: "delete-1",
          pubkey: OWNER,
          created_at: announcement().created_at + 1,
          kind: 5,
          tags: [["a", `30617:${OWNER}:crew`]],
          content: "",
          sig: "signature",
        },
      ],
    },
  );

  assert.equal(content, "@codex inspect.");
});
