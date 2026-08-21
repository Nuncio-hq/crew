import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  displayForgeCheckName,
  summarizeChecksTab,
} from "./forgeCheckGroups.ts";

describe("summarizeChecksTab", () => {
  it("shows completed/total Running while checks are in flight", () => {
    const summary = summarizeChecksTab([
      check({ name: "A", conclusion: "success" }),
      check({ name: "B", conclusion: "skipped" }),
      check({
        name: "C",
        conclusion: "pending",
        status: "IN_PROGRESS",
      }),
      check({ name: "D", conclusion: "pending", status: "QUEUED" }),
    ]);
    assert.equal(summary.kind, "running");
    assert.equal(summary.label, "2/4 Running");
    assert.equal(summary.completed, 2);
  });

  it("shows N Failed when terminal with failures", () => {
    const summary = summarizeChecksTab([
      check({ name: "A", conclusion: "success" }),
      check({ name: "B", conclusion: "failure" }),
    ]);
    assert.equal(summary.kind, "failed");
    assert.equal(summary.label, "1 Failed");
  });

  it("shows N Passed when all green", () => {
    const summary = summarizeChecksTab([
      check({ name: "A", conclusion: "success" }),
      check({ name: "B", conclusion: "neutral" }),
    ]);
    assert.equal(summary.kind, "passed");
    assert.equal(summary.label, "2 Passed");
  });
});

describe("displayForgeCheckName", () => {
  it("composes Workflow / Job when name is bare", () => {
    assert.equal(
      displayForgeCheckName(
        check({
          name: "Desktop Fast",
          workflow: "NuncioCrew CI",
          conclusion: "success",
        }),
      ),
      "NuncioCrew CI / Desktop Fast",
    );
  });

  it("keeps an already-composed name", () => {
    assert.equal(
      displayForgeCheckName(
        check({
          name: "NuncioCrew CI / Desktop Fast",
          workflow: "NuncioCrew CI",
          conclusion: "success",
        }),
      ),
      "NuncioCrew CI / Desktop Fast",
    );
  });
});

function check(input) {
  return {
    name: input.name,
    status: input.status ?? "COMPLETED",
    conclusion: input.conclusion,
    url: null,
    workflow: input.workflow ?? null,
    runId: null,
    startedAt: null,
    completedAt: null,
  };
}
