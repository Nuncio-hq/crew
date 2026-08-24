import assert from "node:assert/strict";
import test from "node:test";

import { normalizeAddToChatSelection } from "./addToChatQuote.ts";

test("normalizes selected message text without flattening paragraphs", () => {
  assert.equal(
    normalizeAddToChatSelection("  First line  \nSecond\u00a0line  "),
    "First line\nSecond line",
  );
});

test("returns an empty string for whitespace-only selections", () => {
  assert.equal(normalizeAddToChatSelection(" \n\t "), "");
});
