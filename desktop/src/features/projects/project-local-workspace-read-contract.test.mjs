import assert from "node:assert/strict";
import { test } from "node:test";

import { projectLocalWorkspaceFromEvent } from "./lib/project-local-workspace.ts";

const OWNER = "a".repeat(64);
const CHANNEL_ID = "018f30b4-57c0-7f10-a3f8-9f7d8e6c5b4a";
const LOCAL_PATH = "/Users/oscar/Projects/Nuncio Crew";

function announcement(extraTags = []) {
  return {
    id: "event-1",
    pubkey: OWNER,
    created_at: 1_753_000_000,
    kind: 30_617,
    tags: [
      ["d", "crew"],
      ["name", "Crew"],
      ["description", "Agent mission control"],
      ["clone", "https://github.com/Nuncio-hq/crew.git"],
      ["web", "https://github.com/Nuncio-hq/crew"],
      ["buzz-channel", CHANNEL_ID],
      ...extraTags,
    ],
    content: "Agent mission control",
    sig: "signature",
  };
}

test("reads the canonical buzz-channel binding from the relay event", () => {
  const workspace = projectLocalWorkspaceFromEvent(announcement());

  assert.equal(workspace.channelId, CHANNEL_ID);
});

test("reads one raw absolute local workspace without changing project identity", () => {
  const event = announcement([["buzz-location", "local", LOCAL_PATH]]);
  const workspace = projectLocalWorkspaceFromEvent(event);

  assert.deepEqual(workspace.localWorkspace, {
    status: "linked",
    path: LOCAL_PATH,
  });
  assert.equal(event.pubkey, OWNER);
  assert.deepEqual(
    event.tags.find((tag) => tag[0] === "d"),
    ["d", "crew"],
  );
  assert.deepEqual(
    event.tags.find((tag) => tag[0] === "clone"),
    ["clone", "https://github.com/Nuncio-hq/crew.git"],
  );
});

test("reports an announcement with no local location as unlinked", () => {
  const workspace = projectLocalWorkspaceFromEvent(announcement());

  assert.deepEqual(workspace.localWorkspace, { status: "unlinked" });
});

test("rejects malformed local locations instead of guessing", () => {
  for (const path of ["", "relative/path", "file:///tmp/crew", "/tmp/\0crew"]) {
    const workspace = projectLocalWorkspaceFromEvent(
      announcement([["buzz-location", "local", path]]),
    );

    assert.deepEqual(
      workspace.localWorkspace,
      { status: "invalid", reason: "invalid-local-path" },
      `path ${JSON.stringify(path)}`,
    );
  }
});

test("reports duplicate local locations as ambiguous", () => {
  const workspace = projectLocalWorkspaceFromEvent(
    announcement([
      ["buzz-location", "local", LOCAL_PATH],
      ["buzz-location", "local", "/Users/oscar/Projects/other"],
    ]),
  );

  assert.deepEqual(workspace.localWorkspace, {
    status: "invalid",
    reason: "duplicate-local-paths",
  });
});

test("ignores other location types and trailing extension fields", () => {
  const workspace = projectLocalWorkspaceFromEvent(
    announcement([
      ["buzz-location", "cache", "/tmp/crew"],
      ["buzz-location", "local", LOCAL_PATH, "future-field"],
    ]),
  );

  assert.deepEqual(workspace.localWorkspace, {
    status: "linked",
    path: LOCAL_PATH,
  });
});
