import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { parseHandoverModel } from "./handoverTag.mjs";

describe("handoverTag (#173)", () => {
  it("parses crew-handover model id", () => {
    assert.equal(
      parseHandoverModel([
        ["h", "channel"],
        ["crew-handover", "gpt-4o-mini"],
      ]),
      "gpt-4o-mini",
    );
  });

  it("returns null without tag or blank model", () => {
    assert.equal(parseHandoverModel([["crew-evidence", "metrics"]]), null);
    assert.equal(parseHandoverModel([["crew-handover", "  "]]), null);
    assert.equal(parseHandoverModel(undefined), null);
  });
});
