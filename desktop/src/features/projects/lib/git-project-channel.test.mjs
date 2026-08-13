import assert from "node:assert/strict";
import { test } from "node:test";

import { gitProjectWorkspaceForChannel } from "./git-project-channel.ts";

const CHANNEL = "018f30b4-57c0-7f10-a3f8-9f7d8e6c5b4a";

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
    owner: "a".repeat(64),
    contributors: [],
    createdAt: 0,
    status: "ready",
    defaultBranch: "main",
    repoAddress: `30617:${"a".repeat(64)}:crew`,
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
    owner: "a".repeat(64),
    createdAt: 0,
    projectChannelId: CHANNEL,
    status: "ready",
    projectAddress: `30621:${"a".repeat(64)}:crew`,
    primaryRepositoryAddress: null,
    repositoryAddresses: [],
    repositories: [repo()],
    legacy: false,
    ...overrides,
  };
}

test("matches a Project channel with a linked git workspace", () => {
  assert.deepEqual(gitProjectWorkspaceForChannel(CHANNEL, [project()]), {
    localPath: "/tmp/crew",
    repoAddress: `30617:${"a".repeat(64)}:crew`,
    defaultBranch: "main",
  });
});

test("matches a legacy 30617 wrapper via repository.channelId", () => {
  assert.deepEqual(
    gitProjectWorkspaceForChannel(CHANNEL, [
      project({ projectChannelId: null }),
    ]),
    {
      localPath: "/tmp/crew",
      repoAddress: `30617:${"a".repeat(64)}:crew`,
      defaultBranch: "main",
    },
  );
});

test("ignores unlinked workspaces and other channels", () => {
  assert.equal(
    gitProjectWorkspaceForChannel(CHANNEL, [
      project({
        repositories: [
          repo({ localWorkspaceStatus: "unlinked", localWorkspacePath: null }),
        ],
      }),
    ]),
    null,
  );
  assert.equal(gitProjectWorkspaceForChannel("other", [project()]), null);
});
