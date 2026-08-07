import assert from "node:assert/strict";
import test from "node:test";

import { parseAgentReceipt } from "./agentReceipt.mjs";

test("maps lights and engineering details", () => {
  const receipt = parseAgentReceipt(
    JSON.stringify({
      summary: "Implemented the change.",
      verify: "Run the scoped checks.",
      lights: [{ label: "Tests", status: "passed" }],
      engineering: {
        pr_ref: "#84",
        branch: "feat/receipt",
        files_changed: ["src/a.ts"],
        ci: [{ label: "CI", status: "green" }],
      },
    }),
  );

  assert.deepEqual(receipt, {
    summary: "Implemented the change.",
    verify: "Run the scoped checks.",
    lights: [{ label: "Tests", status: "passed" }],
    engineering: {
      prRef: "#84",
      branch: "feat/receipt",
      filesChanged: ["src/a.ts"],
      ci: [{ label: "CI", status: "green" }],
    },
  });
});

test("keeps engineering details available for collapsed rendering", () => {
  const receipt = parseAgentReceipt(
    JSON.stringify({
      summary: "Done",
      verify: "Review the diff",
      lights: [],
      engineering: { branch: "main", files_changed: ["a", 2] },
    }),
  );

  assert.deepEqual(receipt?.engineering, {
    prRef: null,
    branch: "main",
    filesChanged: ["a"],
    ci: [],
  });
});

test("returns a graceful fallback model for malformed content", () => {
  assert.equal(parseAgentReceipt("{"), null);
  assert.equal(parseAgentReceipt(JSON.stringify({ summary: "missing" })), null);
});
