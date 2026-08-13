import assert from "node:assert/strict";
import { test } from "node:test";

import {
  DEFAULT_WORKSPACE_BINDING,
  isDefaultWorkspaceBinding,
  parseWorkspaceBindingParams,
  workspaceBindingQuerySuffix,
} from "./workspaceBindingSpec.ts";

test("absent params are today's new-worktree default", () => {
  assert.deepEqual(parseWorkspaceBindingParams(null, null), {
    ws: "new",
    branch: null,
    base: null,
  });
  assert.equal(
    workspaceBindingQuerySuffix(DEFAULT_WORKSPACE_BINDING, "main"),
    "",
  );
  assert.equal(
    isDefaultWorkspaceBinding(DEFAULT_WORKSPACE_BINDING, "main"),
    true,
  );
});

test("ws=main and ws=branch serialize only when non-default", () => {
  assert.equal(
    workspaceBindingQuerySuffix({ mode: "main" }, "main"),
    "&ws=main",
  );
  assert.equal(
    workspaceBindingQuerySuffix({ mode: "branch", name: "feature/x" }, "main"),
    "&ws=branch%3Afeature%2Fx",
  );
});

test("base is omitted when it equals the repo default", () => {
  assert.equal(
    workspaceBindingQuerySuffix({ mode: "new", base: "main" }, "main"),
    "",
  );
  assert.equal(
    workspaceBindingQuerySuffix({ mode: "new", base: "release" }, "main"),
    "&base=release",
  );
});

test("unknown ws fails closed to new worktree", () => {
  assert.deepEqual(parseWorkspaceBindingParams("cowork", null), {
    ws: "new",
    branch: null,
    base: null,
  });
});
