import assert from "node:assert/strict";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { useFeatureEnabled } from "./useFeatureEnabled.ts";
import { OVERRIDES_KEY } from "./store.ts";

function Destinations() {
  return React.createElement(
    "p",
    null,
    `projects:${useFeatureEnabled("projects")};workflows:${useFeatureEnabled("workflows")};pulse:${useFeatureEnabled("pulse")}`,
  );
}
test("fresh installs and old opt-outs retain stable navigation without rewriting preferences", () => {
  for (const overrides of [{}, { projects: false, workflows: false }]) {
    const saved = JSON.stringify(overrides);
    const writes = [];
    globalThis.window = {
      localStorage: {
        getItem: (key) => (key === OVERRIDES_KEY ? saved : null),
        setItem: (...args) => writes.push(args),
      },
    };
    assert.match(
      renderToStaticMarkup(React.createElement(Destinations)),
      /projects:true;workflows:true;pulse:false/,
    );
    assert.deepEqual(writes, []);
  }
});
