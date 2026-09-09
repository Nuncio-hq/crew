import assert from "node:assert/strict";
import test from "node:test";
import {
  attachExistingProjectRepository,
  linkExistingProjectChannel,
  retryProjectChannelLink,
  loadProjectChannelLink,
  ProjectLinkScopeChanged,
} from "./projectChannelLink.ts";
const token = {
  scope: { owner: "a".repeat(64), community: "https://relay.example" },
  workspace_generation: 2,
  identity_generation: 3,
};
const operation = {
  id: "operation",
  revision: 7,
  kind: "project-change",
  resource_key: "30621:owner:project",
  payload: { channel_id: "channel" },
  reconciled: false,
};
function fixture(overrides = {}) {
  const calls = [];
  const native = {
    capture: async () => token,
    list: async () => ({ token, value: [] }),
    load: async () => ({ token, value: operation }),
    prepare: async (...args) => {
      calls.push(["prepare", ...args]);
      return { token, value: { result: "created", operation } };
    },
    dispatch: async (...args) => {
      calls.push(["dispatch", ...args]);
      return {
        token,
        value: { ...operation, status: "complete", reconciled: true },
      };
    },
    ...overrides,
  };
  return { native, calls };
}
test("durable prepare precedes dispatch of the exact native ID/revision", async () => {
  const { native, calls } = fixture();
  const result = await linkExistingProjectChannel(
    operation.resource_key,
    "channel",
    native,
  );
  assert.equal(result.value.reconciled, true);
  assert.deepEqual(calls, [
    ["prepare", token, operation.resource_key, "channel"],
    ["dispatch", token, operation, false],
  ]);
});
test("repository attachment prepares its exact coordinate and dispatches once", async () => {
  const repository = `30617:${"b".repeat(64)}:backend`;
  const calls = [];
  const native = fixture({
    prepareRepository: async (...args) => {
      calls.push(["prepareRepository", ...args]);
      return {
        token,
        value: {
          result: "created",
          operation: {
            ...operation,
            payload: {
              ...operation.payload,
              channel_id: undefined,
              action: {
                type: "attach-repository",
                repository_coordinate: repository,
              },
            },
          },
        },
      };
    },
  }).native;
  native.dispatch = async (...args) => {
    calls.push(["dispatch", ...args]);
    return {
      token,
      value: {
        ...operation,
        payload: {
          ...operation.payload,
          action: {
            type: "attach-repository",
            repository_coordinate: repository,
          },
        },
        status: "complete",
        reconciled: true,
      },
    };
  };
  const result = await attachExistingProjectRepository(
    operation.resource_key,
    repository,
    native,
  );
  assert.equal(result.value.reconciled, true);
  assert.equal(calls[0][0], "prepareRepository");
  assert.equal(calls[0][3], repository);
  assert.equal(calls[1][0], "dispatch");
});
test("identity ABA after prepare prevents dispatch", async () => {
  let reads = 0;
  const { native, calls } = fixture({
    capture: async () =>
      ++reads === 1 ? token : { ...token, identity_generation: 5 },
  });
  await assert.rejects(
    linkExistingProjectChannel(operation.resource_key, "channel", native),
    ProjectLinkScopeChanged,
  );
  assert.equal(calls.length, 1);
});
test("another pending intent cannot be silently retargeted", async () => {
  const { native, calls } = fixture({
    prepare: async () => ({
      token,
      value: {
        result: "existing",
        operation: { ...operation, payload: { channel_id: "other" } },
      },
    }),
  });
  await assert.rejects(
    linkExistingProjectChannel(operation.resource_key, "channel", native),
    /another pending operation/,
  );
  assert.deepEqual(calls, []);
});
test("explicit retry sends the same native record and never prepares again", async () => {
  const { native, calls } = fixture();
  await retryProjectChannelLink(token, operation, native);
  assert.deepEqual(calls, [["dispatch", token, operation, true]]);
});
test("a stale completion is not returned for current-view cache updates", async () => {
  const { native } = fixture({
    dispatch: async () => ({
      token: { ...token, workspace_generation: 9 },
      value: operation,
    }),
  });
  await assert.rejects(
    retryProjectChannelLink(token, operation, native),
    ProjectLinkScopeChanged,
  );
});
test("recovery pagination finds an exact Project resource after the first page", async () => {
  let pages = 0;
  const { native } = fixture({
    list: async () => ({
      token,
      value:
        ++pages === 1
          ? Array.from({ length: 100 }, (_, id) => ({
              id: String(id),
              reconciled: true,
            }))
          : [operation],
    }),
  });
  const result = await loadProjectChannelLink(operation.resource_key, native);
  assert.equal(result.operation, operation);
  assert.equal(pages, 2);
});
