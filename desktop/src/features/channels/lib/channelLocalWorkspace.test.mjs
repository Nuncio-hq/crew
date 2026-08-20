import assert from "node:assert/strict";
import { test } from "node:test";

import { exclusiveChannelLocalWorkspace } from "./channelLocalWorkspace.ts";

const CHANNEL = "018f30b4-57c0-7f10-a3f8-9f7d8e6c5b4a";
const OWNER = "a".repeat(64);
const GENERAL = "general";

function repo(overrides = {}) {
  return {
    id: "repo",
    dtag: "crew",
    name: "crew",
    description: "",
    cloneUrls: [],
    localWorkspacePath: "/tmp/crew",
    localWorkspaceStatus: "linked",
    webUrl: null,
    owner: OWNER,
    contributors: [],
    createdAt: 0,
    status: "ready",
    defaultBranch: "main",
    repoAddress: `30617:${OWNER}:crew`,
    channelId: CHANNEL,
    ...overrides,
  };
}

function project(overrides = {}) {
  return {
    id: "project",
    dtag: "crew",
    name: "crew",
    description: "",
    owner: OWNER,
    createdAt: 0,
    projectChannelId: CHANNEL,
    status: "ready",
    projectAddress: `30621:${OWNER}:crew`,
    primaryRepositoryAddress: null,
    repositoryAddresses: [],
    repositories: [repo()],
    legacy: false,
    ...overrides,
  };
}

test("matches an exclusive Project channel with a linked git workspace", () => {
  assert.deepEqual(exclusiveChannelLocalWorkspace(CHANNEL, [project()]), {
    repoAddress: `30617:${OWNER}:crew`,
    owner: OWNER,
    dtag: "crew",
    localPath: "/tmp/crew",
    workspaceMode: "git",
  });
});

test("matches a legacy 30617 wrapper via exactly one repository.channelId", () => {
  assert.deepEqual(
    exclusiveChannelLocalWorkspace(CHANNEL, [
      project({ projectChannelId: null }),
    ]),
    {
      repoAddress: `30617:${OWNER}:crew`,
      owner: OWNER,
      dtag: "crew",
      localPath: "/tmp/crew",
      workspaceMode: "git",
    },
  );
});

test("includes cowork folder-mode bindings", () => {
  assert.deepEqual(
    exclusiveChannelLocalWorkspace(CHANNEL, [
      project({
        repositories: [repo({ workspaceMode: "folder" })],
      }),
    ]),
    {
      repoAddress: `30617:${OWNER}:crew`,
      owner: OWNER,
      dtag: "crew",
      localPath: "/tmp/crew",
      workspaceMode: "folder",
    },
  );
});

test("returns a still-tagged path even when the folder is gone", () => {
  assert.deepEqual(
    exclusiveChannelLocalWorkspace(CHANNEL, [
      project({
        repositories: [
          repo({ localWorkspacePath: "/Users/gone/old-checkout" }),
        ],
      }),
    ]),
    {
      repoAddress: `30617:${OWNER}:crew`,
      owner: OWNER,
      dtag: "crew",
      localPath: "/Users/gone/old-checkout",
      workspaceMode: "git",
    },
  );
});

test("returns null for #general, no binding, or missing inputs", () => {
  assert.equal(exclusiveChannelLocalWorkspace(null, [project()]), null);
  assert.equal(exclusiveChannelLocalWorkspace(undefined, [project()]), null);
  assert.equal(exclusiveChannelLocalWorkspace(CHANNEL, undefined), null);
  assert.equal(exclusiveChannelLocalWorkspace(CHANNEL, []), null);
  assert.equal(exclusiveChannelLocalWorkspace(GENERAL, [project()]), null);
});

test("returns null for unlinked or missing paths", () => {
  assert.equal(
    exclusiveChannelLocalWorkspace(CHANNEL, [
      project({
        repositories: [
          repo({ localWorkspaceStatus: "unlinked", localWorkspacePath: null }),
        ],
      }),
    ]),
    null,
  );
  assert.equal(
    exclusiveChannelLocalWorkspace(CHANNEL, [
      project({
        repositories: [repo({ localWorkspacePath: "" })],
      }),
    ]),
    null,
  );
  assert.equal(
    exclusiveChannelLocalWorkspace(CHANNEL, [
      project({
        repositories: [repo({ localWorkspacePath: "relative/crew" })],
      }),
    ]),
    null,
  );
});

test("returns null when two repositories bind the same channel", () => {
  assert.equal(
    exclusiveChannelLocalWorkspace(CHANNEL, [
      project({
        repositories: [
          repo({ id: "one", dtag: "one", repoAddress: `30617:${OWNER}:one` }),
          repo({ id: "two", dtag: "two", repoAddress: `30617:${OWNER}:two` }),
        ],
      }),
    ]),
    null,
  );
  assert.equal(
    exclusiveChannelLocalWorkspace(CHANNEL, [
      project({ id: "a", dtag: "a" }),
      project({
        id: "b",
        dtag: "b",
        projectChannelId: null,
        repositories: [repo({ id: "other", dtag: "other" })],
      }),
    ]),
    null,
  );
});
