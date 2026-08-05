import assert from "node:assert/strict";
import { test } from "node:test";

import { eventToProject } from "./hooks.ts";
import {
  hasLocalRepositoryCheckout,
  isProjectLocal,
} from "./lib/projectLocalRepos.ts";

const OWNER = "a".repeat(64);
const CHANNEL_ID = "018f30b4-57c0-7f10-a3f8-9f7d8e6c5b4a";
const LOCAL_PATH = "/Users/oscar/Projects/Nuncio Crew";

function projectEvent(tags) {
  return {
    content: "",
    created_at: 1_753_000_000,
    id: "event-id",
    kind: 30_617,
    pubkey: OWNER,
    sig: "signature",
    tags,
  };
}

test("the Repository read model retains canonical channel and local path without synthesizing clone metadata", () => {
  const repository = eventToProject(
    projectEvent([
      ["d", "nuncio-crew"],
      ["name", "Nuncio Crew"],
      ["buzz-channel", CHANNEL_ID],
      ["buzz-location", "local", LOCAL_PATH],
    ]),
    "https://relay.example",
  );

  assert.equal(repository.localWorkspacePath, LOCAL_PATH);
  assert.equal(repository.localWorkspaceStatus, "linked");
  assert.equal(repository.channelId, CHANNEL_ID);
  assert.deepEqual(repository.cloneUrls, []);
  assert.equal(isProjectLocal(repository, new Set()), true);
  assert.equal(
    hasLocalRepositoryCheckout(repository, new Set(["nuncio-crew"])),
    false,
  );
});

test("invalid duplicate Crew metadata fails closed on the Repository read model", () => {
  const repository = eventToProject(
    projectEvent([
      ["d", "nuncio-crew"],
      ["buzz-channel", CHANNEL_ID],
      ["buzz-channel", "another-channel"],
      ["h", "legacy-channel"],
      ["buzz-location", "local", LOCAL_PATH],
      ["buzz-location", "local", "/tmp/other"],
    ]),
    "https://relay.example",
  );

  assert.equal(repository.localWorkspacePath, null);
  assert.equal(repository.localWorkspaceStatus, "invalid");
  assert.equal(repository.channelId, null);
  assert.deepEqual(repository.cloneUrls, []);
  assert.equal(
    hasLocalRepositoryCheckout(repository, new Set(["nuncio-crew"])),
    false,
  );
  assert.equal(isProjectLocal(repository, new Set(["nuncio-crew"])), false);
});
