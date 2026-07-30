import assert from "node:assert/strict";
import { test } from "node:test";

import {
  buildLocalWorkspaceProject,
  projectNameFromLocalPath,
} from "./lib/project-add-local-workspace.ts";

const CHANNEL_ID = "018f30b4-57c0-7f10-a3f8-9f7d8e6c5b4a";

test("folder basename becomes the default Project name without changing the path", () => {
  const path = "/Users/oscar/Projects/Nuncio Crew 日本語";

  assert.equal(projectNameFromLocalPath(path), "Nuncio Crew 日本語");
  const project = buildLocalWorkspaceProject({
    channelId: CHANNEL_ID,
    localPath: path,
    name: projectNameFromLocalPath(path),
  });

  assert.equal(project.dtag, "nuncio-crew");
  assert.deepEqual(
    project.tags.find(
      (tag) => tag[0] === "buzz-location" && tag[1] === "local",
    ),
    ["buzz-location", "local", path],
  );
});

test("Project creation has one canonical channel and local location but no clone", () => {
  const project = buildLocalWorkspaceProject({
    channelId: CHANNEL_ID,
    localPath: "/Users/oscar/Projects/crew",
    name: "Crew",
  });

  assert.deepEqual(project.tags, [
    ["d", "crew"],
    ["name", "Crew"],
    ["buzz-channel", CHANNEL_ID],
    ["buzz-location", "local", "/Users/oscar/Projects/crew"],
  ]);
  assert.equal(
    project.tags.some((tag) => tag[0] === "clone"),
    false,
  );
});

test("invalid paths and names fail before any relay work", () => {
  for (const localPath of ["", "relative/path", "/tmp/bad\npath"]) {
    assert.throws(
      () =>
        buildLocalWorkspaceProject({
          channelId: CHANNEL_ID,
          localPath,
          name: "Crew",
        }),
      /absolute local folder path/i,
    );
  }
  assert.throws(
    () =>
      buildLocalWorkspaceProject({
        channelId: CHANNEL_ID,
        localPath: "/Users/oscar/Projects/crew",
        name: "---",
      }),
    /letters or numbers/i,
  );
});
