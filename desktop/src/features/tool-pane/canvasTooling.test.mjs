import assert from "node:assert/strict";
import test from "node:test";

import {
  parseCanvasTooling,
  toolingHasUdid,
  writeCanvasTooling,
} from "./canvasTooling.ts";

const ORIGINAL = `Founder guidance.

\`\`\`crew
assignments:
  old: Research
definitions:
  Research: Read first.
routing:
  review: Research
tooling:
  simulator:
    deviceType: iPhone 16 Pro
    runtime: iOS 18
  extraUnknown: keep-me
\`\`\`

Closing notes.`;

test("parseCanvasTooling reads intent", () => {
  const tooling = parseCanvasTooling(ORIGINAL);
  assert.equal(tooling?.simulator?.deviceType, "iPhone 16 Pro");
  assert.equal(tooling?.simulator?.runtime, "iOS 18");
  assert.equal(toolingHasUdid(tooling ?? {}), false);
});

test("writeCanvasTooling preserves roles", () => {
  const next = writeCanvasTooling(ORIGINAL, {
    simulator: { deviceType: "iPhone 16", runtime: "iOS 18" },
    devServer: { command: "pnpm dev --port $PORT", readyPattern: "Local:" },
  });
  assert.match(next, /Research/);
  assert.match(next, /pnpm dev --port \$PORT/);
  assert.match(next, /routing:/);
  assert.equal(next.toLowerCase().includes("udid"), false);
});
