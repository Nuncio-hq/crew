import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

import { buildChannelLocalWorkspaceChipView } from "./ChannelLocalWorkspaceChip.tsx";

const OWNER = "a".repeat(64);
const OTHER = "b".repeat(64);

const BINDING = {
  repoAddress: `30617:${OWNER}:crew`,
  owner: OWNER,
  dtag: "crew",
  localPath: "/Users/gone/old-checkout",
  workspaceMode: "git",
};

test("chip view is hidden when there is no exclusive binding", () => {
  assert.equal(
    buildChannelLocalWorkspaceChipView({
      binding: null,
      currentPubkey: OWNER,
    }),
    null,
  );
});

test("chip view shows a truncated path for a bound workspace", () => {
  const view = buildChannelLocalWorkspaceChipView({
    binding: BINDING,
    currentPubkey: OWNER,
  });
  assert.ok(view);
  assert.equal(view.fullPath, "/Users/gone/old-checkout");
  assert.equal(view.pathLabel, "…/gone/old-checkout");
  assert.notEqual(view.pathLabel, view.fullPath);
});

test("owner sees Relink folder by default and Pick folder when the path is known gone", () => {
  const linked = buildChannelLocalWorkspaceChipView({
    binding: BINDING,
    currentPubkey: OWNER,
  });
  assert.equal(linked?.actionLabel, "Relink folder");
  assert.equal(linked?.goneMessage, null);

  const gone = buildChannelLocalWorkspaceChipView({
    binding: BINDING,
    currentPubkey: OWNER,
    pathMissing: true,
  });
  assert.equal(gone?.actionLabel, "Pick folder");
  assert.equal(
    gone?.goneMessage,
    "The Project folder is gone. Pick a workspace again.",
  );
});

test("non-owner sees the path and no Relink button", () => {
  const view = buildChannelLocalWorkspaceChipView({
    binding: BINDING,
    currentPubkey: OTHER,
  });
  assert.ok(view);
  assert.equal(view.pathLabel, "…/gone/old-checkout");
  assert.equal(view.actionLabel, null);
  assert.equal(view.goneMessage, null);
});

test("chip click reuses the existing folder picker and 30617 publisher", async () => {
  const source = await readFile(
    new URL("./ChannelLocalWorkspaceChip.tsx", import.meta.url),
    "utf8",
  );

  assert.match(source, /chooseProjectWorkspaceFolder/);
  assert.match(source, /linkCurrentProjectWorkspace/);
  assert.match(
    source,
    /Project workspace linked\. Send a new message to use it\./,
  );
  assert.match(source, /if \(!path\) return/);
  assert.match(source, /owner,/);
  assert.match(source, /currentPubkey/);
  assert.match(source, /dtag:/);
  assert.match(source, /channelId/);
  assert.match(source, /localPath: path/);
  assert.match(source, /\["crew-project-announcement"\]/);
  assert.match(source, /projectsQueryKey/);
  assert.doesNotMatch(source, /probeProjectGitWorkspace/);
  assert.doesNotMatch(source, /CrewProjectWorkspacePanel/);
});

test("ChannelPane mounts the chip beside the composer workspace selector", async () => {
  const pane = await readFile(
    new URL("./ChannelPane.tsx", import.meta.url),
    "utf8",
  );
  assert.match(pane, /<ChannelLocalWorkspaceChip/);
  assert.match(pane, /toolbarExtraActions/);
  assert.equal(pane.includes("onSelectProjects"), false);
});

test("projects screen still does not mount the Local workspace strip", async () => {
  const screen = await readFile(
    new URL("../../projects/ui/crew-projects-screen.tsx", import.meta.url),
    "utf8",
  );
  assert.equal(screen.includes("CrewProjectWorkspacePanel"), false);
});
