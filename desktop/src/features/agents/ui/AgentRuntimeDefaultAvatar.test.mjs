import assert from "node:assert/strict";
import test from "node:test";

import {
  resolveAgentDefaultRuntimeId,
  runtimeIconStub,
} from "./AgentRuntimeDefaultAvatar.tsx";

test("resolveAgentDefaultRuntimeId prefers agent pin over persona", () => {
  assert.equal(
    resolveAgentDefaultRuntimeId({
      agentRuntime: "cursor",
      personaRuntime: "goose",
    }),
    "cursor",
  );
});

test("resolveAgentDefaultRuntimeId falls back to persona", () => {
  assert.equal(
    resolveAgentDefaultRuntimeId({
      agentRuntime: null,
      personaRuntime: "cursor",
    }),
    "cursor",
  );
});

test("resolveAgentDefaultRuntimeId ignores custom and blank", () => {
  assert.equal(
    resolveAgentDefaultRuntimeId({
      agentRuntime: "custom",
      personaRuntime: "  ",
    }),
    null,
  );
  assert.equal(
    resolveAgentDefaultRuntimeId({
      agentRuntime: "",
      personaRuntime: "custom",
    }),
    null,
  );
});

test("runtimeIconStub trims id", () => {
  assert.deepEqual(runtimeIconStub("  cursor  "), { id: "cursor" });
});
