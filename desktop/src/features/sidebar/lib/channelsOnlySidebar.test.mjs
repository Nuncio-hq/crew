import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";

import { streamChannelsForSidebar } from "./sidebarRailContract.ts";

const here = path.dirname(fileURLToPath(import.meta.url));

function readNearby(rel) {
  return readFileSync(path.join(here, rel), "utf8");
}

describe("sidebar is Inbox + channels + DMs (#223)", () => {
  it("keeps project-bound office channels in the Channels list", () => {
    const listed = streamChannelsForSidebar(
      [
        { id: "engineering", channelType: "stream" },
        { id: "general", channelType: "stream" },
        { id: "alice-dm", channelType: "dm" },
      ],
      new Set(["engineering"]),
    );
    assert.deepEqual(
      listed.map((channel) => channel.id),
      ["engineering", "general"],
    );
  });

  it("does not declare a Projects or Workbench primary-nav item", () => {
    const source = readNearby("../ui/AppSidebarPinnedHeader.tsx");
    assert.equal(
      source.includes("open-projects-view"),
      false,
      "top-nav Projects only reopened the project list",
    );
    assert.equal(
      source.includes("onSelectProjects"),
      false,
      "Projects is not a sidebar peer",
    );
    assert.equal(
      source.includes("open-workbench-view"),
      false,
      "Workbench nav is gone",
    );
  });

  it("does not mount a Projects work-tree rail or hide those channels", () => {
    const block = readNearby("../../work-tree/ui/WorkTreeSidebarBlock.tsx");
    const sidebar = readNearby("../ui/AppSidebar.tsx");
    assert.equal(
      block.includes("WorkTreeSection"),
      false,
      "Projects folder tree is not a sidebar rail",
    );
    assert.equal(
      sidebar.includes("useProjectFolderChannelIds"),
      false,
      "project-bound channels stay in Channels",
    );
  });
});
