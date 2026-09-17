import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { AGENT_ACTIVITY_CHROME } from "./agentActivityChrome.ts";

describe("agent activity chrome", () => {
  it("names the scope of every stop control", () => {
    // Three different blast radii share one screen. A label that does not say
    // which one it is turns a per-run Stop into an accidental thread-wide one.
    assert.equal(AGENT_ACTIVITY_CHROME.stopAllRuns, "Stop all runs");
    assert.equal(AGENT_ACTIVITY_CHROME.stopRun, "Stop run");
    assert.equal(AGENT_ACTIVITY_CHROME.stopSelectedRun, "Stop selected run");
    assert.equal(AGENT_ACTIVITY_CHROME.stopAgent("Hermes"), "Stop Hermes");
    assert.equal(
      new Set([
        AGENT_ACTIVITY_CHROME.stopAllRuns,
        AGENT_ACTIVITY_CHROME.stopRun,
        AGENT_ACTIVITY_CHROME.stopSelectedRun,
        AGENT_ACTIVITY_CHROME.stopAgent("Hermes"),
        AGENT_ACTIVITY_CHROME.stop,
      ]).size,
      5,
    );
  });

  it("gives the two steer inputs different accessible names", () => {
    // The compact thread input and the Activity input can be open at once;
    // one shared name would present two targets as one.
    assert.notEqual(
      AGENT_ACTIVITY_CHROME.steerThisRunLabel,
      AGENT_ACTIVITY_CHROME.steerSelectedRunLabel,
    );
  });

  it("counts runs, not agents, and steers by name when agent-scoped", () => {
    assert.equal(AGENT_ACTIVITY_CHROME.runsWorking(1), "1 run");
    assert.equal(AGENT_ACTIVITY_CHROME.runsWorking(2), "2 runs");
    assert.equal(
      AGENT_ACTIVITY_CHROME.agentWorkingHint("Hermes"),
      "Hermes is working — steer if stuck",
    );
    assert.equal(AGENT_ACTIVITY_CHROME.steerAgent("Hermes"), "Steer Hermes");
  });
});
