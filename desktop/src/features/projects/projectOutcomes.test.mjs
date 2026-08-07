import assert from "node:assert/strict";
import { test } from "node:test";

import {
  deriveProjectOutcomeCards,
  partitionProjectCrew,
  projectShipLog,
} from "./projectOutcomes.ts";
import {
  ingestApprovalRequest,
  resetNeedsYouStore,
} from "../agents/needsYouStore.ts";

const OWNER = "a".repeat(64);
const AGENT = "b".repeat(64);
const UPSTREAM = "c".repeat(64);
const CHANNEL = "channel-1";

test.beforeEach(() => resetNeedsYouStore());

function project(overrides = {}) {
  return {
    id: "30621:owner:alpha",
    dtag: "alpha",
    name: "Alpha",
    owner: OWNER,
    projectChannelId: CHANNEL,
    repositoryAddresses: ["30617:owner:repo"],
    repositories: [{ repoAddress: "30617:owner:repo", channelId: CHANNEL }],
    ...overrides,
  };
}

test("derives scoped outcome counts and keeps a repo-less project first-class", () => {
  const cards = deriveProjectOutcomeCards(
    [
      project(),
      project({
        id: "30621:owner:empty",
        dtag: "empty",
        name: "Empty",
        projectChannelId: null,
        repositoryAddresses: [],
        repositories: [],
      }),
    ],
    [{ channelId: CHANNEL, conversationId: "thread-1" }],
    [
      {
        conversationId: "thread-2",
        channelId: CHANNEL,
        outcome: "completed",
        endedAt: 1_000,
      },
    ],
    [{ projectId: "30621:owner:alpha", status: "Merged", mergedAt: 2_000 }],
    2_100,
  );

  assert.deepEqual(
    cards.find((card) => card.project.dtag === "alpha")?.counts,
    {
      needsYou: 0,
      ready: 1,
      inFlight: 1,
      shipped30d: 1,
    },
  );
  assert.deepEqual(
    cards.find((card) => card.project.dtag === "empty")?.counts,
    {
      needsYou: 0,
      ready: 0,
      inFlight: 0,
      shipped30d: 0,
    },
  );
});

test("sorts needs-you, ready, in-flight and dims quiet projects", () => {
  ingestApprovalRequest({
    id: "needs-request",
    channelId: "needs",
    rootEventId: "needs-root",
    conversationId: "needs-thread",
    agentPubkey: AGENT,
    createdAt: Date.now(),
  });
  const cards = deriveProjectOutcomeCards(
    [
      project({
        id: "quiet",
        dtag: "quiet",
        projectChannelId: null,
        repositoryAddresses: [],
        repositories: [],
      }),
      project({ id: "flight", dtag: "flight", projectChannelId: "flight" }),
      project({
        id: "ready",
        dtag: "ready",
        name: "Zulu",
        projectChannelId: "ready",
      }),
      project({
        id: "needs",
        dtag: "needs",
        name: "Alpha",
        projectChannelId: "needs",
      }),
    ],
    [{ channelId: "flight", conversationId: "flight-thread" }],
    [
      {
        conversationId: "needs-thread",
        channelId: "needs",
        outcome: "error",
        endedAt: 1_000,
      },
      {
        conversationId: "ready-thread",
        channelId: "ready",
        outcome: "completed",
        endedAt: 1_000,
      },
      {
        conversationId: "needs-thread",
        channelId: "needs",
        outcome: "completed",
        endedAt: 1_000,
      },
    ],
    [],
    2_000,
  );

  assert.deepEqual(
    cards.map((card) => card.project.dtag),
    ["needs", "ready", "flight", "quiet"],
  );
  assert.equal(cards.at(-1)?.quiet, true);
  assert.equal(
    cards.find((card) => card.project.dtag === "needs")?.counts.needsYou,
    1,
  );
});

test("uses project-owned repository addresses for orphan PR belonging", () => {
  const cards = deriveProjectOutcomeCards(
    [
      project({
        id: "project-a",
        dtag: "a",
        repositoryAddresses: ["30617:owner:a"],
      }),
      project({
        id: "project-b",
        dtag: "b",
        repositoryAddresses: ["30617:owner:b"],
      }),
    ],
    [],
    [],
    [
      {
        repoAddress: "30617:owner:a",
        status: "Merged",
        mergedAt: 1_900,
        author: AGENT,
      },
      {
        repoAddress: "30617:owner:orphan",
        status: "Merged",
        mergedAt: 1_900,
        author: AGENT,
      },
    ],
    2_000,
  );

  assert.equal(
    cards.find((card) => card.project.id === "project-a")?.counts.shipped30d,
    1,
  );
  assert.equal(
    cards.find((card) => card.project.id === "project-b")?.counts.shipped30d,
    0,
  );
});

test("needs-you prevents a project from being quiet", () => {
  ingestApprovalRequest({
    id: "needs-request",
    channelId: "needs",
    rootEventId: "needs-root",
    conversationId: "needs-thread",
    agentPubkey: AGENT,
    createdAt: Date.now(),
  });

  const [updatedCard] = deriveProjectOutcomeCards(
    [
      project({
        id: "needs",
        dtag: "needs",
        projectChannelId: "needs",
        repositoryAddresses: [],
      }),
    ],
    [],
    [],
    [],
    2_000,
  );

  assert.equal(updatedCard.counts.needsYou, 1);
  assert.equal(updatedCard.quiet, false);
});

test("maps merged pull requests to a newest-first ship log", () => {
  const log = projectShipLog([
    {
      id: "old",
      title: "Older",
      author: OWNER,
      status: "Merged",
      updatedAt: 100,
      statusCreatedAt: 100,
    },
    {
      id: "open",
      title: "Open",
      author: AGENT,
      status: "Open",
      updatedAt: 300,
    },
    {
      id: "new",
      title: "Newest",
      author: AGENT,
      status: "Merged",
      updatedAt: 200,
    },
  ]);

  assert.deepEqual(
    log.map(({ title, author, mergedAt }) => ({ title, author, mergedAt })),
    [
      { title: "Newest", author: AGENT, mergedAt: 200 },
      { title: "Older", author: OWNER, mergedAt: 100 },
    ],
  );
});

test("partitions project crew from upstream contributors", () => {
  const result = partitionProjectCrew(
    [OWNER, AGENT, UPSTREAM],
    new Set([UPSTREAM]),
  );

  assert.deepEqual(result.crew, [OWNER, AGENT]);
  assert.deepEqual(result.upstream, [UPSTREAM]);
});
