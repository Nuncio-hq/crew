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
  //
  // Two independent user-visible properties (do not collapse into one order scan):
  // 1) resolve failure → draft is NOT cleared (throw skips clearComposer)
  // 2) no-upload success → clearComposer runs after resolve, before await send
  //    (atomic persistent audience; e2e can see composer change before send resolves)
  const source = await readFile(
    new URL("../messages/ui/useMentionSendComplete.ts", import.meta.url),
    "utf8",
  );
  const finishSendIndex = source.indexOf("const finishSend = async");
  assert.ok(finishSendIndex >= 0);

  const resolverIndex = source.indexOf(
    "await resolveCurrentProjectChannelAgentMessage",
    finishSendIndex,
  );
  assert.ok(resolverIndex >= 0);

  const failureMessageIndex = source.indexOf(
    "Could not resolve Project workspace",
    resolverIndex,
  );
  assert.ok(failureMessageIndex >= 0);
  assert.ok(failureMessageIndex > resolverIndex);

  const failureThrowIndex = source.indexOf(
    "throw new Error(message",
    failureMessageIndex,
  );
  assert.ok(failureThrowIndex >= 0);
  assert.ok(failureThrowIndex > failureMessageIndex);

  // Independent of success-path ordering: the resolve→throw slice must not
  // clear the draft. Order-only asserts on the happy path would pass even if
  // this failure branch wiped content.
  const resolveToThrowSlice = source.slice(resolverIndex, failureThrowIndex);
  assert.equal(
    /clearComposer(?:AfterPreflight)?\(/.test(resolveToThrowSlice),
    false,
    "resolve-fail path must not call clearComposer before throw",
  );

  const clearAfterResolveIndex = source.indexOf(
    "clearAfterResolve &&",
    failureThrowIndex,
  );
  assert.ok(clearAfterResolveIndex >= 0);
  assert.ok(clearAfterResolveIndex > failureThrowIndex);

  const clearComposerIndex = source.indexOf(
    "clearComposerAfterPreflight(",
    clearAfterResolveIndex,
  );
  assert.ok(clearComposerIndex >= 0);
  assert.ok(clearComposerIndex > clearAfterResolveIndex);

  const networkSendIndex = source.indexOf("await send(", clearComposerIndex);
  assert.ok(networkSendIndex >= 0);
  assert.ok(networkSendIndex > clearComposerIndex);

  const noUploadFinishCall = source.indexOf(
    "await finishSend([], undefined, true)",
    networkSendIndex,
  );
  assert.ok(noUploadFinishCall >= 0);
  assert.ok(noUploadFinishCall > networkSendIndex);

  // No-upload path must not clear again after finishSend returns.
  const clearAfterNoUploadCall = source.indexOf(
    "clearComposerAfterPreflight(",
    noUploadFinishCall,
  );
  assert.equal(clearAfterNoUploadCall, -1);
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
