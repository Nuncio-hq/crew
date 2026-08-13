import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { KIND_AGENT_RECEIPT } from "../../../shared/constants/kinds.ts";
import {
  isOfficeVisibleRow,
  isRoleCheckObserverItem,
} from "./workbenchOfficeFilter.ts";

const headId = "1".repeat(64);

function messageRow(overrides = {}) {
  return {
    type: "message",
    id: overrides.id ?? "m1",
    createdAt: 1,
    message: {
      id: overrides.id ?? "m1",
      createdAt: 1,
      author: "owner",
      time: "",
      body: overrides.body ?? "hello",
      depth: 0,
      kind: overrides.kind,
      tags: overrides.tags ?? [],
    },
  };
}

describe("workbenchOfficeFilter", () => {
  it("keeps kickoff, receipts, evidence, questions, and catch-up", () => {
    assert.equal(isOfficeVisibleRow(messageRow({ id: headId }), headId), true);
    assert.equal(
      isOfficeVisibleRow(
        messageRow({
          id: "e1",
          tags: [["crew-evidence", "test-run"]],
        }),
        headId,
      ),
      true,
    );
    assert.equal(
      isOfficeVisibleRow(
        messageRow({ id: "r1", kind: KIND_AGENT_RECEIPT }),
        headId,
      ),
      true,
    );
    assert.equal(
      isOfficeVisibleRow(
        {
          type: "user-input",
          id: "q1",
          createdAt: 2,
          item: { event: { id: "q1" }, request: {} },
        },
        headId,
      ),
      true,
    );
    assert.equal(
      isOfficeVisibleRow(
        { type: "catch-up", id: "catch-up", createdAt: 3 },
        headId,
      ),
      true,
    );
  });

  it("hides tool calls, ROLE-CHECK, and sleep/wake lines", () => {
    assert.equal(
      isOfficeVisibleRow(
        messageRow({ id: "chat", body: "status update" }),
        headId,
      ),
      false,
    );
    assert.equal(
      isOfficeVisibleRow(
        {
          type: "observer",
          id: "tool",
          createdAt: 2,
          agentPubkey: "aa".repeat(32),
          item: { id: "t", type: "tool", renderClass: "shell", title: "bash" },
        },
        headId,
      ),
      false,
    );
    assert.equal(
      isOfficeVisibleRow(
        {
          type: "sleep-wake",
          id: "s",
          createdAt: 2,
          agentPubkey: "aa".repeat(32),
          kind: "sleep",
          label: "Sleeping",
        },
        headId,
      ),
      false,
    );
    assert.equal(
      isRoleCheckObserverItem({
        renderClass: "thought",
        title: "ROLE-CHECK",
        text: "confirming owner",
      }),
      true,
    );
  });
});
