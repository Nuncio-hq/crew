import assert from "node:assert/strict";
import test from "node:test";

import {
  AUXILIARY_PANEL_CONTRACT_MIN_PX,
  AUXILIARY_PANEL_NARROW_PX,
  DECLARED_PLANS_RAIL_WIDTH_PX,
  DECLARED_PLANS_SIDE_BY_SIDE_MIN_PX,
  RESPONSIVE_SURFACE_CONTRACT,
  isNarrowPane,
  shouldStackDeclaredPlansRail,
} from "./responsiveContract.ts";

test("contract table covers every issued surface", () => {
  const names = RESPONSIVE_SURFACE_CONTRACT.map((row) => row.surface);
  for (const required of [
    "App window",
    "Sidebar",
    "Auxiliary/thread panel",
    "Focus drawer",
    "Tool Pane",
    "PR hub",
    "Wiki TOC rail",
    "Composer",
    "Huddle companion",
  ]) {
    assert.ok(names.includes(required), `missing ${required}`);
  }
});

test("declared-plans rail stacks whenever a side column would squeeze", () => {
  assert.equal(shouldStackDeclaredPlansRail(300), true);
  assert.equal(shouldStackDeclaredPlansRail(340), true);
  assert.equal(shouldStackDeclaredPlansRail(380), true);
  assert.equal(
    shouldStackDeclaredPlansRail(DECLARED_PLANS_SIDE_BY_SIDE_MIN_PX),
    false,
  );
  assert.equal(shouldStackDeclaredPlansRail(720), false);
  assert.ok(
    DECLARED_PLANS_SIDE_BY_SIDE_MIN_PX > DECLARED_PLANS_RAIL_WIDTH_PX,
    "side-by-side needs leftover readable column",
  );
});

test("narrow pane threshold matches the 340px empty-state / meta collapse", () => {
  assert.equal(isNarrowPane(AUXILIARY_PANEL_CONTRACT_MIN_PX), true);
  assert.equal(isNarrowPane(AUXILIARY_PANEL_NARROW_PX), true);
  assert.equal(isNarrowPane(380), false);
  assert.equal(isNarrowPane(0), false);
});
