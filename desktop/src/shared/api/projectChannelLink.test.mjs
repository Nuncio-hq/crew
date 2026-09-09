import assert from "node:assert/strict";
import test from "node:test";
import {
  attachExistingProjectRepository,
  linkProjectWorkspace,
  retryProjectWorkspaceLink,
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

test("workspace linking persists and dispatches the exact path and channel", async () => {
  const repository = `30617:${"b".repeat(64)}:backend`;
  const channel = "018f30b4-57c0-7f10-a3f8-9f7d8e6c5b4a";
  const path = "/Users/oscar/Projects/Nuncio Crew";
  const calls = [];
  const workspaceOperation = {
    ...operation,
    resource_key: repository,
    payload: {
      action: {
        type: "link-workspace",
        repository_coordinate: repository,
        channel_id: channel,
        local_path: path,
      },
    },
  };
  const native = fixture({
    prepareWorkspace: async (...args) => {
      calls.push(["prepareWorkspace", ...args]);
      return {
        token,
        value: { result: "created", operation: workspaceOperation },
      };
    },
    dispatch: async (...args) => {
      calls.push(["dispatch", ...args]);
      return {
        token,
        value: {
          ...workspaceOperation,
          status: "complete",
          reconciled: true,
        },
      };
    },
  }).native;
  const result = await linkProjectWorkspace(repository, channel, path, native);
  assert.equal(result.value.reconciled, true);
  assert.deepEqual(calls, [
    ["prepareWorkspace", token, repository, channel, path],
    ["dispatch", token, workspaceOperation, false],
  ]);
});

test("workspace retry rejects a changed path and never prepares a successor", async () => {
  const repository = `30617:${"b".repeat(64)}:backend`;
  const channel = "018f30b4-57c0-7f10-a3f8-9f7d8e6c5b4a";
  const path = "/Users/oscar/Projects/Nuncio Crew";
  const { native, calls } = fixture({
    dispatch: async (...args) => {
      calls.push(["dispatch", ...args]);
      return { token, value: operation };
    },
  });
  const pending = {
    ...operation,
    resource_key: repository,
    payload: {
      action: {
        type: "link-workspace",
        repository_coordinate: repository,
        channel_id: channel,
        local_path: path,
      },
    },
  };
  await assert.rejects(
    retryProjectWorkspaceLink(
      token,
      pending,
      repository,
      channel,
      `${path}/changed`,
      native,
    ),
    /another operation/,
  );
  assert.deepEqual(calls, []);
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
