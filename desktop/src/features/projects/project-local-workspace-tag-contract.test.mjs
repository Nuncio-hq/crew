import assert from "node:assert/strict";
import { test } from "node:test";

import {
  linkProjectWorkspaceTags,
  localWorkspacePrivacyNotice,
  replaceLocalWorkspaceTag,
  validateLocalWorkspacePath,
} from "./lib/project-local-workspace.ts";

const CHANNEL_ID = "018f30b4-57c0-7f10-a3f8-9f7d8e6c5b4a";
const LOCAL_PATH = "/Users/oscar/Projects/Nuncio Crew";

test("linking a Buzz-created Project adds only canonical Crew metadata", () => {
  const existing = [
    ["d", "crew"],
    ["name", "Crew"],
    ["description", "Agent mission control"],
    ["clone", "https://github.com/Nuncio-hq/crew.git"],
    ["web", "https://github.com/Nuncio-hq/crew"],
  ];
  const tags = linkProjectWorkspaceTags(existing, {
    channelId: CHANNEL_ID,
    localPath: LOCAL_PATH,
  });

  assert.deepEqual(tags, [
    ["d", "crew"],
    ["name", "Crew"],
    ["description", "Agent mission control"],
    ["clone", "https://github.com/Nuncio-hq/crew.git"],
    ["web", "https://github.com/Nuncio-hq/crew"],
    ["buzz-channel", CHANNEL_ID],
    ["buzz-location", "local", LOCAL_PATH],
  ]);
  assert.equal(existing.length, 5);
});

test("relink changes only the local location and preserves future metadata", () => {
  const original = [
    ["d", "crew"],
    ["name", "Crew"],
    ["clone", "https://github.com/Nuncio-hq/crew.git"],
    ["web", "https://github.com/Nuncio-hq/crew"],
    ["buzz-channel", CHANNEL_ID],
    ["buzz-protect", "maintainers"],
    ["buzz-location", "cache", "/tmp/crew"],
    ["future-tag", "future-value", "future-marker"],
    ["buzz-location", "local", "/Users/oscar/Projects/old"],
  ];

  const next = replaceLocalWorkspaceTag(original, LOCAL_PATH);

  assert.deepEqual(next, [
    ["d", "crew"],
    ["name", "Crew"],
    ["clone", "https://github.com/Nuncio-hq/crew.git"],
    ["web", "https://github.com/Nuncio-hq/crew"],
    ["buzz-channel", CHANNEL_ID],
    ["buzz-protect", "maintainers"],
    ["buzz-location", "cache", "/tmp/crew"],
    ["future-tag", "future-value", "future-marker"],
    ["buzz-location", "local", LOCAL_PATH],
  ]);
  assert.deepEqual(original.at(-1), [
    "buzz-location",
    "local",
    "/Users/oscar/Projects/old",
  ]);
});

test("relink collapses duplicate local records to one explicit value", () => {
  const next = replaceLocalWorkspaceTag(
    [
      ["d", "crew"],
      ["buzz-location", "local", "/tmp/one"],
      ["buzz-location", "local", "/tmp/two"],
    ],
    LOCAL_PATH,
  );

  assert.deepEqual(
    next.filter((tag) => tag[0] === "buzz-location" && tag[1] === "local"),
    [["buzz-location", "local", LOCAL_PATH]],
  );
});

test("linking fails closed for duplicate or malformed canonical channels", () => {
  for (const existing of [
    [
      ["d", "crew"],
      ["buzz-channel", CHANNEL_ID],
      ["buzz-channel", "another-channel"],
    ],
    [
      ["d", "crew"],
      ["buzz-channel", ""],
    ],
  ]) {
    assert.throws(
      () =>
        linkProjectWorkspaceTags(existing, {
          channelId: CHANNEL_ID,
          localPath: LOCAL_PATH,
        }),
      /canonical Project channel/i,
    );
  }
});

test("an existing canonical channel cannot be silently replaced", () => {
  assert.throws(
    () =>
      linkProjectWorkspaceTags(
        [
          ["d", "crew"],
          ["buzz-channel", CHANNEL_ID],
        ],
        {
          channelId: "another-channel",
          localPath: LOCAL_PATH,
        },
      ),
    /canonical Project channel/i,
  );
});

test("path validation accepts raw POSIX absolute paths with spaces", () => {
  assert.equal(validateLocalWorkspacePath(LOCAL_PATH), LOCAL_PATH);
  assert.equal(
    validateLocalWorkspacePath("/Users/oscar/Đồ án Crew"),
    "/Users/oscar/Đồ án Crew",
  );
  assert.equal(validateLocalWorkspacePath("/"), "/");
});

test("path validation rejects values that are not raw absolute paths", () => {
  for (const path of [
    "",
    " ",
    "relative/path",
    "./relative",
    "file:///tmp/crew",
    "https://example.com/crew",
    "/tmp/\0crew",
  ]) {
    assert.throws(
      () => validateLocalWorkspacePath(path),
      /absolute local folder/i,
      JSON.stringify(path),
    );
  }
});

test("privacy copy says the raw path is plaintext relay metadata", () => {
  const copy = localWorkspacePrivacyNotice("ws://localhost:3000");

  assert.match(copy, /raw local path/i);
  assert.match(copy, /plaintext/i);
  assert.match(copy, /signed Project event/i);
  assert.match(copy, /ws:\/\/localhost:3000/);
});
