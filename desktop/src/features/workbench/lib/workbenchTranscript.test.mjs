import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { firstUnreadAfterReadAt } from "./workbenchCatchUp.ts";
import {
  buildWorkbenchTranscript,
  observerBelongsToThread,
  userInputBelongsToThread,
} from "./workbenchTranscript.ts";

const ROOT = "1".repeat(64);
const CHANNEL = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const AGENT = "aa".repeat(32);

function message(id, createdAt, body = "hi") {
  return {
    id,
    createdAt,
    author: "owner",
    time: "",
    body,
    depth: 0,
    tags: [],
  };
}

describe("workbenchCatchUp", () => {
  it("marks the first message newer than the thread read frontier", () => {
    assert.equal(
      firstUnreadAfterReadAt(
        [
          message(ROOT, 10),
          message("a".repeat(64), 20),
          message("b".repeat(64), 30),
        ],
        15,
      ),
      "a".repeat(64),
    );
    assert.equal(firstUnreadAfterReadAt([message(ROOT, 10)], null), null);
    assert.equal(firstUnreadAfterReadAt([message(ROOT, 10)], 50), null);
  });
});

describe("buildWorkbenchTranscript", () => {
  it("interleaves messages, questions, and observer rows by timestamp", () => {
    const questionEvent = {
      id: "4".repeat(64),
      created_at: 15,
      kind: 46040,
      pubkey: AGENT,
      content: "{}",
      sig: "",
      tags: [["e", ROOT, "", "reply"]],
    };
    const rows = buildWorkbenchTranscript({
      channelId: CHANNEL,
      conversationId: "conv",
      catchUpAfterId: "a".repeat(64),
      threadRootId: ROOT,
      messages: [message(ROOT, 10), message("a".repeat(64), 20)],
      userInputs: [{ event: questionEvent, request: { questions: [] } }],
      observerByAgent: [
        {
          agentPubkey: AGENT,
          items: [
            {
              id: "tool-1",
              type: "tool",
              renderClass: "shell",
              title: "bash",
              timestamp: "1970-01-01T00:00:12.000Z",
              channelId: CHANNEL,
              conversationId: "conv",
            },
          ],
        },
      ],
      sleepWake: [
        {
          agentPubkey: AGENT,
          kind: "sleep",
          label: "Hermes is sleeping · wakes on mention",
        },
      ],
    });
    assert.deepEqual(
      rows.map((row) => row.type),
      [
        "message",
        "observer",
        "user-input",
        "catch-up",
        "message",
        "sleep-wake",
      ],
    );
  });

  it("scopes questions and observer frames to the thread", () => {
    const other = {
      id: "5".repeat(64),
      created_at: 1,
      kind: 46040,
      pubkey: AGENT,
      content: "{}",
      sig: "",
      tags: [
        ["e", "2".repeat(64), "", "root"],
        ["e", "2".repeat(64), "", "reply"],
      ],
    };
    assert.equal(userInputBelongsToThread(other, ROOT), false);
    assert.equal(
      observerBelongsToThread(
        { channelId: CHANNEL, conversationId: "other" },
        CHANNEL,
        "conv",
      ),
      false,
    );
  });
});
