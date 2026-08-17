import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  channelHrefFromWorkbench,
  parseWorkbenchLens,
  workbenchHref,
} from "./workbenchRoutes.ts";

const CHANNEL = "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9";
const ROOT = "1".repeat(64);

describe("workbenchRoutes", () => {
  it("defaults the lens to By thread and encodes office as a search flag", () => {
    assert.equal(parseWorkbenchLens("agent"), "agent");
    assert.equal(parseWorkbenchLens("thread"), "thread");
    assert.equal(parseWorkbenchLens("nope"), "thread");
    assert.equal(workbenchHref(), "/");
    assert.equal(
      workbenchHref(CHANNEL, ROOT, { lens: "agent", office: true }),
      `/channels/${CHANNEL}?thread=${ROOT}`,
    );
  });

  it("exits to the same channel thread the workbench opened", () => {
    assert.equal(
      channelHrefFromWorkbench(CHANNEL, ROOT),
      `/channels/${CHANNEL}?thread=${ROOT}`,
    );
  });
});
