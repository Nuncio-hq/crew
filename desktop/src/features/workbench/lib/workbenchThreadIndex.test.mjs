import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  deriveWorkbenchThreadIndex,
  ensureSelectedWorkbenchRow,
  findWorkbenchRow,
  groupWorkbenchByAgent,
  groupWorkbenchByChannel,
  missionStateToStatus,
} from "./workbenchThreadIndex.ts";

const CHANNEL_A = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const CHANNEL_B = "94a444a4-c0a3-5966-ab05-530c6ddc2301";
const ROOT_A = "1".repeat(64);
const ROOT_B = "2".repeat(64);
const HERMES = "aa".repeat(32);
const CODEX = "bb".repeat(32);

describe("deriveWorkbenchThreadIndex", () => {
  it("projects mission rows + inbox items without minting new identity", () => {
    const rows = deriveWorkbenchThreadIndex({
      channels: [
        { id: CHANNEL_A, name: "engineering" },
        { id: CHANNEL_B, name: "design" },
      ],
      agentNamesByPubkey: new Map([
        [HERMES, "Hermes"],
        [CODEX, "Codex"],
      ]),
      sleepingAgentPubkeys: new Set([CODEX]),
      missionRows: [
        {
          conversationId: "c1",
          channelId: CHANNEL_A,
          threadTitle: "Fix reconnect freeze",
          agentPubkey: HERMES,
          state: "working",
          phaseOrHeadline: "editing",
          age: 1,
          inboxItem: null,
          rootEventId: ROOT_A,
          messageEventId: ROOT_A,
        },
        {
          conversationId: "c2",
          channelId: CHANNEL_B,
          threadTitle: "Landing revamp",
          agentPubkey: CODEX,
          state: "working",
          phaseOrHeadline: "idle",
          age: 2,
          inboxItem: null,
          rootEventId: ROOT_B,
          messageEventId: ROOT_B,
        },
      ],
      inboxItems: [],
    });
    assert.equal(rows.length, 2);
    assert.equal(rows[0].status, "working");
    assert.equal(rows[0].channelName, "engineering");
    assert.equal(rows[1].status, "sleeping");
    assert.equal(rows[1].agents[0].name, "Codex");

    const byChannel = groupWorkbenchByChannel(rows);
    assert.deepEqual(
      byChannel.map((group) => group.channelName),
      ["design", "engineering"],
    );
    const byAgent = groupWorkbenchByAgent(rows);
    assert.equal(byAgent.length, 2);
    assert.equal(
      byAgent.find((group) => group.name === "Hermes")?.threads.length,
      1,
    );
    assert.equal(
      byAgent.find((group) => group.name === "Codex")?.status,
      "sleeping",
    );
  });

  it("keeps the open thread on the rail even when it is not in inbox yet", () => {
    const rows = ensureSelectedWorkbenchRow([], {
      channelId: CHANNEL_A,
      channelName: "engineering",
      threadRootId: ROOT_A,
    });
    assert.equal(rows.length, 1);
    assert.equal(
      findWorkbenchRow(rows, CHANNEL_A, ROOT_A)?.title,
      "Open thread",
    );
    assert.equal(
      ensureSelectedWorkbenchRow(rows, {
        channelId: CHANNEL_A,
        channelName: "engineering",
        threadRootId: ROOT_A,
      }).length,
      1,
    );
  });

  it("maps mission states onto rail status dots", () => {
    assert.equal(missionStateToStatus("needsYou", false), "needs-you");
    assert.equal(missionStateToStatus("working", true), "sleeping");
    assert.equal(missionStateToStatus("readyToReview", false), "ready");
    assert.equal(missionStateToStatus("failed", false), "failed");
  });
});
